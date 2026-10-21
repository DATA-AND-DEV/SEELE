//! **A ligação do navegador**: o aperto de mão e o fluxo de controle de uma
//! sessão, sem rede (Tarefa 2.6 da F4).
//!
//! A página abre a WebTransport e o fluxo bidirecional de controle, e daí em
//! diante só carrega bytes: os que chegam do fluxo entram em
//! [`Ligacao::chegou`], e os que saem ela tira de [`Ligacao::para_o_fio`] e
//! escreve. Quanto pesa um quadro, que carimbo ele leva, o que se responde a um
//! desafio e o que uma resposta quer dizer moram aqui, no mesmo `seele-proto`
//! do desktop: **nenhuma linha do protocolo em JavaScript**
//! (`specs/06-clientes-gui.md:19`).
//!
//! O aperto é o do desktop (`seele_core::client::handshake` e `concluir`):
//! o `Hello` com a mínima desta build, o carimbo da primeira resposta conferido
//! antes do `decode` (ADR 0058), a prova sobre a impressão **que a conexão
//! conferiu** (o `fp=` do link, que é o certificado longo: medição 3 do ADR
//! 0057), o anúncio de MODs comparado com o que já foi aceito, e a sessão.
//!
//! **A chave não passa por aqui.** A identidade do navegador é uma chave
//! Ed25519 da WebCrypto que não se exporta (`identidade-web.js`): a ligação
//! devolve os bytes a assinar ([`Passo::Assinar`]), a página assina, e a
//! assinatura volta por [`Ligacao::assinado`]. É o S2 do adendo de 08/10/2026,
//! §9, do lado de cá: o que se assina é a mensagem que o `seele-proto` monta,
//! e nunca bytes vindos do fio.
//!
//! Uma recusa sai no JSON do `seele_ffi::ConnectionError`, com as mesmas
//! regras do `classify_connect_failure` da FFI: a casca diz a mesma frase nas
//! duas, e a conformidade (`dois_produtores_de_recusa.rs`) confere.

use seele_proto::control::{
    decode, decode_da_lista, encode_em, ClientMessage, ControlError, Lido, ServerMessage,
    MAX_NICKNAME_LEN, NONCE_LEN,
};
use seele_proto::mods::ModAnunciado;
use seele_proto::quadro::{enquadrar, Desenquadrador};
use seele_proto::version::{
    confere_o_carimbo, linha_de, linha_do_protocolo, Linha, FAIXA, VERSAO_MINIMA,
};
use seele_sessao::retrato::EndReason;
use seele_sessao::SessionInfo;
use serde_json::{json, Value};

use crate::sessao::Sessao;

/// Como o navegador se apresenta no `Hello`: `navegador/` e o número do
/// pacote, quando quem empacota o põe, e nunca o do `Cargo.toml` (o mesmo
/// cuidado do `quem_sou_eu` do desktop); sem ele, `navegador/local`.
#[must_use]
pub fn quem_sou_eu() -> String {
    format!(
        "navegador/{}",
        option_env!("SEELE_VERSAO").unwrap_or("local")
    )
}

/// O que a página sabe antes de discar.
#[derive(Debug, Clone)]
pub struct Pedido {
    /// O apelido digitado.
    pub apelido: String,
    /// A chave pública Ed25519 desta identidade.
    pub chave_publica: [u8; 32],
    /// A impressão do certificado longo, o `fp=` do link: é a que o navegador
    /// conferiu, pelos hashes dos curtos que o mesmo link levou.
    pub impressao: [u8; 32],
    /// O segredo do convite, quando a entrada é por convite.
    pub convite: Option<String>,
    /// O conjunto de MODs que esta pessoa já aceitou neste servidor.
    pub aceito: Option<String>,
}

impl Pedido {
    /// O pedido no JSON da página: `{"apelido", "chave_publica", "impressao",
    /// "convite", "aceito"}`, com a chave e a impressão em hex (64 dígitos) e
    /// os dois últimos podendo faltar.
    ///
    /// # Errors
    ///
    /// O texto que não é esse objeto, numa frase para o registro.
    pub fn de_json(texto: &str) -> Result<Self, String> {
        let lido: Value = serde_json::from_str(texto).map_err(|erro| erro.to_string())?;
        let texto_de = |campo: &str| -> Result<Option<String>, String> {
            match lido.get(campo) {
                None | Some(Value::Null) => Ok(None),
                Some(Value::String(valor)) => Ok(Some(valor.clone())),
                Some(_) => Err(format!("`{campo}` não é texto")),
            }
        };
        let trinta_e_dois = |campo: &str| -> Result<[u8; 32], String> {
            let hex = texto_de(campo)?.ok_or_else(|| format!("falta `{campo}`"))?;
            bytes_do_hex(&hex).ok_or_else(|| format!("`{campo}` não tem 32 bytes em hex"))
        };
        Ok(Self {
            apelido: texto_de("apelido")?.ok_or("falta `apelido`")?,
            chave_publica: trinta_e_dois("chave_publica")?,
            impressao: trinta_e_dois("impressao")?,
            convite: texto_de("convite")?,
            aceito: texto_de("aceito")?,
        })
    }
}

/// 32 bytes de 64 dígitos hex, em maiúsculas ou minúsculas.
fn bytes_do_hex(hex: &str) -> Option<[u8; 32]> {
    if hex.len() != 64 || !hex.is_ascii() {
        return None;
    }
    let mut bytes = [0_u8; 32];
    for (byte, par) in bytes.iter_mut().zip(hex.as_bytes().chunks(2)) {
        *byte = u8::from_str_radix(std::str::from_utf8(par).ok()?, 16).ok()?;
    }
    Some(bytes)
}

/// Em que ponto do aperto a ligação está.
#[derive(Debug)]
enum Etapa {
    /// O `Hello` saiu; a primeira resposta é o desafio.
    Desafio,
    /// O desafio chegou; a página está assinando.
    Assinando,
    /// A prova saiu; vem a sessão, o anúncio de MODs ou uma recusa.
    Resposta,
    /// A sessão está aberta.
    Dentro(Box<Sessao>),
    /// Acabou, por recusa ou por erro: nada mais se lê.
    Fim,
}

/// O que a página tem de fazer depois de um pedaço de bytes.
#[derive(Debug, Clone, PartialEq)]
pub enum Passo {
    /// Assinar estes bytes com a chave da identidade, e devolver a assinatura
    /// por [`Ligacao::assinado`].
    Assinar(Vec<u8>),
    /// A sessão abriu: o retrato já se lê.
    Entrou,
    /// Uma mensagem dobrada: ela em JSON, e os eventos que a casca ouve.
    Chegou {
        /// A mensagem, no JSON do `ServerMessage`.
        mensagem: Value,
        /// Os eventos, no JSON do `seele_ffi::Event`.
        eventos: Vec<Value>,
    },
    /// Uma mensagem de um vocabulário mais novo: pulada, como no desktop.
    Desconhecida {
        /// O ordinal que veio.
        ordinal: u32,
    },
    /// A entrada não aconteceu: o motivo no JSON do `ConnectionError`. Depois
    /// dele, a página escreve o que estiver em [`Ligacao::para_o_fio`], fecha
    /// o envio e espera um pouco antes de largar a conexão (a recusa que não
    /// chega não é uma recusa: `seele_core::client::concluir`).
    Recusada(Value),
    /// O fluxo de controle se perdeu (um quadro torto ou grande demais): a
    /// sessão acabou. O motivo, para o registro.
    Perdida(String),
}

impl Passo {
    /// O passo no JSON que a página lê: `{"assinar": [bytes]}`, `"entrou"`,
    /// `{"chegou": mensagem, "eventos": […]}`, `{"desconhecida": N}`,
    /// `{"recusada": ConnectionError}` ou `{"perdida": texto}`.
    #[must_use]
    pub fn em_json(&self) -> Value {
        match self {
            Self::Assinar(bytes) => json!({ "assinar": bytes }),
            Self::Entrou => json!("entrou"),
            Self::Chegou { mensagem, eventos } => json!({ "chegou": mensagem, "eventos": eventos }),
            Self::Desconhecida { ordinal } => json!({ "desconhecida": ordinal }),
            Self::Recusada(motivo) => json!({ "recusada": motivo }),
            Self::Perdida(motivo) => json!({ "perdida": motivo }),
        }
    }
}

/// O aperto de mão e a sessão de uma conexão do navegador.
#[derive(Debug)]
pub struct Ligacao {
    etapa: Etapa,
    apelido: String,
    impressao: [u8; 32],
    aceito: Option<String>,
    /// O número que toda escrita leva: a mínima até a primeira resposta, e o
    /// carimbo dela depois.
    carimbo: u8,
    entrada: Desenquadrador,
    saida: Vec<u8>,
    medida: Medida,
}

/// O sinal desta ponta: o `Signal` do `seele-proto`, alimentado a cada volta
/// como o `medir_a_volta` da FFI o alimenta.
#[derive(Debug, Default)]
struct Medida {
    sinal: seele_proto::signal::Signal,
    rtt_micros: u64,
    jitter_ms: f32,
}

impl Ligacao {
    /// Abre a ligação: o `Hello` fica esperando em [`Ligacao::para_o_fio`].
    ///
    /// # Errors
    ///
    /// O apelido que o fio não leva sai no JSON do
    /// `ConnectionError::ApelidoGrandeDemais`, **antes de discar**, como no
    /// desktop (lote C55 da F5). Um `Hello` que não passa na validação sai
    /// como `Unreachable`, que é o que o desktop diria de um `Hello` que não
    /// saiu.
    pub fn abrir(pedido: Pedido) -> Result<Self, Value> {
        if pedido.apelido.len() > MAX_NICKNAME_LEN {
            return Err(json!({ "ApelidoGrandeDemais": { "limite_bytes": MAX_NICKNAME_LEN } }));
        }
        let mut ligacao = Self {
            etapa: Etapa::Desafio,
            apelido: pedido.apelido.clone(),
            impressao: pedido.impressao,
            aceito: pedido.aceito,
            carimbo: VERSAO_MINIMA,
            entrada: Desenquadrador::default(),
            saida: Vec::new(),
            medida: Medida::default(),
        };
        ligacao
            .escrever(&ClientMessage::Hello {
                faixa: FAIXA,
                apis_de_mod: seele_proto::mods::APIS_ACEITAS.to_vec(),
                client: quem_sou_eu(),
                nickname: pedido.apelido,
                public_key: pedido.chave_publica.to_vec(),
                join_secret: pedido.convite,
                material_e2ee: None,
            })
            .map_err(|_| json!("Unreachable"))?;
        Ok(ligacao)
    }

    /// Os bytes que esperam para ir ao fluxo de controle, já enquadrados. Sai
    /// tudo de uma vez, e a fila fica vazia.
    pub fn para_o_fio(&mut self) -> Vec<u8> {
        std::mem::take(&mut self.saida)
    }

    /// A sessão, depois do [`Passo::Entrou`].
    #[must_use]
    pub fn sessao(&self) -> Option<&Sessao> {
        match &self.etapa {
            Etapa::Dentro(sessao) => Some(sessao),
            _ => None,
        }
    }

    /// A sessão, para os comandos que mexem nas escolhas desta ponta.
    pub fn sessao_mut(&mut self) -> Option<&mut Sessao> {
        match &mut self.etapa {
            Etapa::Dentro(sessao) => Some(sessao),
            _ => None,
        }
    }

    /// Uma volta da medida: o tempo de ida e volta que o transporte estimou, o
    /// jitter de chegada da voz e a pior perda que ela contou. Diz se o que a
    /// casca mostra mudou (o `TelemetryChanged`), pela regra do
    /// `medir_a_volta` da FFI: o sinal, o tempo de ida e volta por mais de um
    /// milissegundo, ou o jitter.
    pub fn medir(&mut self, rtt_micros: u64, jitter_ms: f32, perda: f32) -> bool {
        #[allow(
            clippy::cast_precision_loss,
            reason = "um tempo de ida e volta em microssegundos cabe folgado no f32"
        )]
        let rtt_ms = rtt_micros as f32 / 1000.0;
        let antes = self.medida.sinal.value();
        let sinal = self.medida.sinal.update(seele_proto::signal::SyncInputs {
            rtt_ms,
            jitter_ms,
            loss_fraction: perda,
        });
        let rtt_antes = std::mem::replace(&mut self.medida.rtt_micros, rtt_micros);
        let jitter_antes = std::mem::replace(&mut self.medida.jitter_ms, jitter_ms);
        sinal != antes
            || rtt_antes.abs_diff(rtt_micros) > 1_000
            || (jitter_antes - jitter_ms).abs() >= 0.1
    }

    /// A telemetria que esta ponta mede, com a perda que o servidor contou:
    /// os campos do `Telemetry` do `Snapshot` que não são do áudio.
    #[must_use]
    pub fn telemetria(&self, perda_contada: f32) -> Value {
        #[allow(
            clippy::cast_precision_loss,
            reason = "um tempo de ida e volta em microssegundos cabe folgado no f32"
        )]
        let rtt_ms = self.medida.rtt_micros as f32 / 1000.0;
        let sinal = self.medida.sinal.value();
        json!({
            "rtt_ms": rtt_ms,
            "jitter_ms": self.medida.jitter_ms,
            "loss_fraction": perda_contada,
            "signal": sinal,
            "sync_band": seele_sessao::retrato::SignalBand::from(
                seele_proto::signal::SignalBand::of(sinal)
            ),
        })
    }

    /// O carimbo desta sessão.
    #[must_use]
    pub const fn carimbo(&self) -> u8 {
        self.carimbo
    }

    /// Uma mensagem do cliente, para o fio, com o carimbo da sessão.
    ///
    /// # Errors
    ///
    /// A mensagem que não passa na validação do `seele-proto`, ou que não cabe
    /// num quadro.
    pub fn escrever(&mut self, mensagem: &ClientMessage) -> Result<(), ControlError> {
        let quadro = encode_em(self.carimbo, mensagem)?;
        self.saida.extend(enquadrar(&quadro)?);
        Ok(())
    }

    /// A assinatura da página sobre os bytes do [`Passo::Assinar`].
    ///
    /// Fora da hora (sem desafio pendente), não faz nada: a página não manda
    /// prova que ninguém pediu.
    pub fn assinado(&mut self, assinatura: &[u8]) -> Vec<Passo> {
        if !matches!(self.etapa, Etapa::Assinando) {
            return Vec::new();
        }
        let resposta = ClientMessage::Response {
            proof: assinatura.to_vec(),
        };
        if self.escrever(&resposta).is_err() {
            return self.recusar(json!("Unreachable"));
        }
        self.etapa = Etapa::Resposta;
        Vec::new()
    }

    /// Bytes do fluxo de controle, como chegaram: em qualquer pedaço.
    pub fn chegou(&mut self, bytes: &[u8]) -> Vec<Passo> {
        self.entrada.empurrar(bytes);
        let mut passos = Vec::new();
        loop {
            if matches!(self.etapa, Etapa::Fim) {
                return passos;
            }
            match self.entrada.proximo() {
                Ok(Some(quadro)) => passos.extend(self.um_quadro(&quadro)),
                Ok(None) => return passos,
                Err(erro) => {
                    passos.extend(self.perder(&erro));
                    return passos;
                }
            }
        }
    }

    /// O fluxo de controle acabou sem recusa nenhuma: no aperto, é o
    /// `Unreachable` do desktop («no Challenge came back»); dentro, a sessão
    /// caiu.
    pub fn fluxo_acabou(&mut self) -> Vec<Passo> {
        match self.etapa {
            Etapa::Fim => Vec::new(),
            Etapa::Dentro(_) => {
                self.etapa = Etapa::Fim;
                vec![Passo::Perdida("o fluxo de controle acabou".into())]
            }
            Etapa::Desafio | Etapa::Assinando | Etapa::Resposta => {
                self.recusar(json!("Unreachable"))
            }
        }
    }

    fn um_quadro(&mut self, quadro: &[u8]) -> Vec<Passo> {
        match std::mem::replace(&mut self.etapa, Etapa::Fim) {
            Etapa::Desafio => self.desafio(quadro),
            Etapa::Assinando => {
                // O servidor não fala antes da prova: o desktop nem leria.
                self.recusar(classificar_protocolo())
            }
            Etapa::Resposta => match decode::<ServerMessage>(quadro) {
                Ok(resposta) => self.resposta(resposta),
                Err(_) => self.recusar(json!("Unreachable")),
            },
            Etapa::Dentro(mut sessao) => {
                let passo = match decode_da_lista::<ServerMessage>(quadro) {
                    Ok(Lido::Mensagem(mensagem)) => {
                        let eventos = sessao.dobrar_com_eventos(&mensagem);
                        serde_json::to_value(&mensagem)
                            .map(|mensagem| Passo::Chegou { mensagem, eventos })
                            .map_err(|erro| erro.to_string())
                    }
                    Ok(Lido::Desconhecida { ordinal }) => Ok(Passo::Desconhecida { ordinal }),
                    Err(erro) => Err(format!("{erro:?}")),
                };
                match passo {
                    Ok(passo) => {
                        self.etapa = Etapa::Dentro(sessao);
                        vec![passo]
                    }
                    Err(motivo) => vec![Passo::Perdida(motivo)],
                }
            }
            Etapa::Fim => Vec::new(),
        }
    }

    /// A primeira resposta: o carimbo antes do `decode`, e o desafio.
    fn desafio(&mut self, quadro: &[u8]) -> Vec<Passo> {
        let Some(&numero) = quadro.first() else {
            return self.recusar(json!("Unreachable"));
        };
        match confere_o_carimbo(numero) {
            Ok(carimbo) => self.carimbo = carimbo,
            Err(_) => return self.recusar(outra_linha(numero)),
        }
        match decode::<ServerMessage>(quadro) {
            Ok(ServerMessage::Challenge { nonce }) => {
                // O nonce tem exatamente 32 bytes, ou o cliente não assina.
                let Ok(nonce) = <[u8; NONCE_LEN]>::try_from(nonce.as_slice()) else {
                    return self.recusar(classificar_protocolo());
                };
                self.etapa = Etapa::Assinando;
                vec![Passo::Assinar(seele_proto::assinatura::prova_de_cliente(
                    &self.impressao,
                    &nonce,
                ))]
            }
            Ok(ServerMessage::Disconnecting { reason }) => self.recusar(recusada(reason.into())),
            Ok(_) => self.recusar(classificar_protocolo()),
            Err(_) => self.recusar(json!("Unreachable")),
        }
    }

    /// Depois da prova: o anúncio de MODs, a sessão, ou a recusa.
    fn resposta(&mut self, resposta: ServerMessage) -> Vec<Passo> {
        match resposta {
            ServerMessage::ModsExigidos { mods, conjunto } => {
                if self.aceito.as_deref() == Some(conjunto.as_str()) {
                    if self
                        .escrever(&ClientMessage::AceitarMods {
                            conjunto: conjunto.clone(),
                        })
                        .is_err()
                    {
                        return self.recusar(json!("Unreachable"));
                    }
                    // Aceito uma vez: um segundo anúncio na mesma ligação não
                    // se aceita sozinho.
                    self.aceito = None;
                    self.etapa = Etapa::Resposta;
                    Vec::new()
                } else {
                    // **Recusa explícita, e não sumir**: quem hospeda distingue
                    // «leu e não quis» de «a conexão caiu».
                    let _ = self.escrever(&ClientMessage::RecusarMods);
                    self.recusar(mods_nao_aceitos(&mods, &conjunto))
                }
            }
            ServerMessage::Session {
                id,
                person,
                ssrc,
                server,
                voice_rooms,
                channels,
                permissions,
                ..
            } => {
                let info = SessionInfo {
                    id,
                    person,
                    ssrc,
                    server,
                    voice_rooms,
                    channels,
                    permissions,
                };
                self.etapa = Etapa::Dentro(Box::new(Sessao::adotar(&info, &self.apelido)));
                vec![Passo::Entrou]
            }
            ServerMessage::Disconnecting { reason } => self.recusar(recusada(reason.into())),
            _ => self.recusar(classificar_protocolo()),
        }
    }

    fn recusar(&mut self, motivo: Value) -> Vec<Passo> {
        self.etapa = Etapa::Fim;
        vec![Passo::Recusada(motivo)]
    }

    fn perder(&mut self, erro: &ControlError) -> Vec<Passo> {
        match self.etapa {
            Etapa::Dentro(_) => {
                self.etapa = Etapa::Fim;
                vec![Passo::Perdida(format!("{erro:?}"))]
            }
            _ => self.recusar(json!("Unreachable")),
        }
    }
}

/// `Refused { reason }`, no JSON do `ConnectionError`.
fn recusada(motivo: EndReason) -> Value {
    json!({ "Refused": { "reason": motivo } })
}

/// A violação de protocolo do desktop: `Refused { reason: Incompatible }`.
fn classificar_protocolo() -> Value {
    recusada(EndReason::Incompatible)
}

/// `OutraLinha`, com a versão do servidor quando ela é anterior à 1.0 e o
/// aviso de que é mais nova: as regras do `classify_connect_failure`.
fn outra_linha(numero: u8) -> Value {
    let linha = linha_de(numero);
    let servidor = match linha {
        Some(Linha::AntesDaUm { .. }) => linha_do_protocolo(numero),
        _ => None,
    };
    json!({ "OutraLinha": {
        "numero": numero,
        "servidor": servidor,
        "mais_nova": matches!(linha, Some(Linha::Posterior { .. })),
    } })
}

/// `ModsNaoAceitos`, com os MODs no JSON do `ModExigido` (o anúncio sem a
/// `api`).
fn mods_nao_aceitos(mods: &[ModAnunciado], conjunto: &str) -> Value {
    let mods: Vec<Value> = mods
        .iter()
        .map(|anunciado| {
            json!({
                "id": anunciado.id,
                "version": anunciado.version,
                "hash": anunciado.hash,
                "repo": anunciado.repo,
                "reach": anunciado.reach,
                "no_servidor": anunciado.no_servidor,
                "tema": anunciado.tema,
            })
        })
        .collect();
    json!({ "ModsNaoAceitos": { "mods": mods, "conjunto": conjunto } })
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used, clippy::expect_used, clippy::indexing_slicing)]

    use super::*;
    use seele_proto::control::{decode as decodificar, DisconnectReason, Permission};
    use seele_proto::ids::{PersonId, SessionId, Ssrc};
    use seele_proto::version::PROTOCOL_VERSION;

    fn pedido() -> Pedido {
        Pedido {
            apelido: "marcela".into(),
            chave_publica: [5; 32],
            impressao: [7; 32],
            convite: Some("segredo".into()),
            aceito: None,
        }
    }

    /// Os quadros que a ligação escreveu, decodificados.
    fn escritos(ligacao: &mut Ligacao) -> Vec<(u8, ClientMessage)> {
        let mut leitor = Desenquadrador::default();
        leitor.empurrar(&ligacao.para_o_fio());
        let mut quadros = Vec::new();
        while let Some(quadro) = leitor.proximo().unwrap() {
            quadros.push((quadro[0], decodificar::<ClientMessage>(&quadro).unwrap()));
        }
        quadros
    }

    fn do_servidor(mensagem: &ServerMessage) -> Vec<u8> {
        enquadrar(&encode_em(PROTOCOL_VERSION, mensagem).unwrap()).unwrap()
    }

    fn sessao() -> ServerMessage {
        ServerMessage::Session {
            id: SessionId(41),
            person: PersonId(7),
            ssrc: Ssrc(700),
            server: "Casa".into(),
            voice_rooms: Vec::new(),
            channels: Vec::new(),
            roles: Vec::new(),
            permissions: vec![Permission::Speak],
        }
    }

    #[test]
    fn o_hello_sai_com_a_minima_e_a_prova_e_a_do_proto() {
        let mut ligacao = Ligacao::abrir(pedido()).unwrap();
        let hello = escritos(&mut ligacao);
        assert_eq!(hello.len(), 1);
        assert_eq!(hello[0].0, VERSAO_MINIMA);
        let ClientMessage::Hello {
            nickname,
            public_key,
            join_secret,
            ..
        } = &hello[0].1
        else {
            panic!("não saiu um Hello: {:?}", hello[0].1);
        };
        assert_eq!(nickname, "marcela");
        assert_eq!(public_key, &vec![5; 32]);
        assert_eq!(join_secret.as_deref(), Some("segredo"));

        // O desafio chega em dois pedaços, e a ligação espera o quadro inteiro.
        let desafio = do_servidor(&ServerMessage::Challenge { nonce: vec![3; 32] });
        let (comeco, resto) = desafio.split_at(3);
        assert!(ligacao.chegou(comeco).is_empty());
        let passos = ligacao.chegou(resto);
        assert_eq!(
            passos,
            vec![Passo::Assinar(seele_proto::assinatura::prova_de_cliente(
                &[7; 32], &[3; 32]
            ))]
        );
        assert!(ligacao.assinado(&[9; 64]).is_empty());
        let prova = escritos(&mut ligacao);
        assert_eq!(
            prova,
            vec![(
                PROTOCOL_VERSION,
                ClientMessage::Response { proof: vec![9; 64] }
            )]
        );

        assert_eq!(ligacao.chegou(&do_servidor(&sessao())), vec![Passo::Entrou]);
        assert_eq!(ligacao.sessao().unwrap().retrato().nickname, "marcela");
    }

    #[test]
    fn o_pedido_da_pagina_e_lido_em_hex_e_o_torto_e_dito() {
        let lido = Pedido::de_json(&format!(
            r#"{{"apelido":"ana","chave_publica":"{}","impressao":"{}","convite":null}}"#,
            "0A".repeat(32),
            "ff".repeat(32)
        ))
        .unwrap();
        assert_eq!(lido.chave_publica, [10; 32]);
        assert_eq!(lido.impressao, [255; 32]);
        assert_eq!(lido.convite, None);
        assert_eq!(lido.aceito, None);
        for torto in [
            r#"{"apelido":"ana"}"#.to_owned(),
            format!(
                r#"{{"apelido":"ana","chave_publica":"{}","impressao":"{}"}}"#,
                "0".repeat(63),
                "f".repeat(64)
            ),
            format!(
                r#"{{"apelido":"ana","chave_publica":"{}","impressao":"{}"}}"#,
                "zz".repeat(32),
                "f".repeat(64)
            ),
            format!(
                r#"{{"apelido":1,"chave_publica":"{}","impressao":"{}"}}"#,
                "0".repeat(64),
                "f".repeat(64)
            ),
            "[]".to_owned(),
        ] {
            assert!(Pedido::de_json(&torto).is_err(), "{torto}");
        }
    }

    #[test]
    fn o_carimbo_de_outra_linha_e_conferido_antes_do_decode() {
        let mut ligacao = Ligacao::abrir(pedido()).unwrap();
        let passos = ligacao.chegou(&enquadrar(&[7, 0, 0]).unwrap());
        let [Passo::Recusada(motivo)] = passos.as_slice() else {
            panic!("não recusou: {passos:?}");
        };
        assert_eq!(motivo["OutraLinha"]["numero"], 7);
    }

    #[test]
    fn a_recusa_do_servidor_leva_o_motivo() {
        let mut ligacao = Ligacao::abrir(pedido()).unwrap();
        let passos = ligacao.chegou(&do_servidor(&ServerMessage::Disconnecting {
            reason: DisconnectReason::NicknameTaken,
        }));
        assert_eq!(
            passos,
            vec![Passo::Recusada(
                json!({ "Refused": { "reason": "NicknameTaken" } })
            )]
        );
    }

    #[test]
    fn o_apelido_grande_demais_nem_disca() {
        let mut pedido = pedido();
        pedido.apelido = "x".repeat(MAX_NICKNAME_LEN + 1);
        assert_eq!(
            Ligacao::abrir(pedido).unwrap_err(),
            json!({ "ApelidoGrandeDemais": { "limite_bytes": MAX_NICKNAME_LEN } })
        );
    }

    #[test]
    fn mods_aceitos_seguem_e_os_outros_sao_recusados_com_nome() {
        let c1 = seele_proto::mods::hex(&seele_proto::mods::identidade_do_conjunto(&mut []));
        let c0 = "0".repeat(64);
        let anuncio = ServerMessage::ModsExigidos {
            mods: Vec::new(),
            conjunto: c1.clone(),
        };
        for (aceito, aceita) in [(Some(c1.clone()), true), (Some(c0), false), (None, false)] {
            let mut pedido = pedido();
            pedido.aceito = aceito;
            let mut ligacao = Ligacao::abrir(pedido).unwrap();
            let _ = escritos(&mut ligacao);
            let _ = ligacao.chegou(&do_servidor(&ServerMessage::Challenge {
                nonce: vec![3; 32],
            }));
            let _ = ligacao.assinado(&[9; 64]);
            let _ = escritos(&mut ligacao);
            let passos = ligacao.chegou(&do_servidor(&anuncio));
            let saiu = escritos(&mut ligacao);
            if aceita {
                assert!(passos.is_empty(), "{passos:?}");
                assert_eq!(
                    saiu,
                    vec![(
                        PROTOCOL_VERSION,
                        ClientMessage::AceitarMods {
                            conjunto: c1.clone()
                        }
                    )]
                );
                assert_eq!(ligacao.chegou(&do_servidor(&sessao())), vec![Passo::Entrou]);
            } else {
                assert_eq!(saiu, vec![(PROTOCOL_VERSION, ClientMessage::RecusarMods)]);
                assert_eq!(
                    passos,
                    vec![Passo::Recusada(
                        json!({ "ModsNaoAceitos": { "mods": [], "conjunto": c1 } })
                    )]
                );
            }
        }
    }
}
