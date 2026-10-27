//! Contrato independente de sinalização pessoal 1:1. Não transporta chat ou mídia.
//!
//! O ponto encaminha pedidos assinados intactos. A assinatura NÃO atesta o
//! ponto: cada participante verifica chave, convite, destinatário, chamada,
//! geração e sequência antes de usar uma oferta. A criptografia fica no
//! consumidor; este módulo limita entradas e monta os bytes canônicos.

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

/// Teto do documento HTTP, antes de desserializar qualquer campo.
pub const MAX_PEDIDO: usize = 24 * 1024;
/// Teto de uma resposta com até oito pedidos originais.
pub const MAX_RESPOSTA: usize = 256 * 1024;
/// Teto da descrição de sessão do navegador.
pub const MAX_SDP: usize = 16 * 1024;
/// Teto de um candidato ICE.
pub const MAX_CANDIDATO: usize = 2048;
/// Uma requisição assinada só é reutilizável durante um minuto.
pub const VALIDADE_DO_PEDIDO: u64 = 60;
/// Validade máxima de um convite novo, em segundos. Não encerra sessão aceita.
pub const VALIDADE_DO_CONVITE: u64 = 24 * 60 * 60;

/// Assinatura Ed25519 em duas partes fixas (serde sem alocação variável).
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Assinatura {
    /// Primeiros 32 bytes.
    pub primeira: [u8; 32],
    /// Últimos 32 bytes.
    pub segunda: [u8; 32],
}

impl Assinatura {
    /// Divide os bytes produzidos pelo assinador Ed25519.
    #[must_use]
    pub fn de_bytes(bytes: [u8; 64]) -> Self {
        let mut assinatura = Self::default();
        let (primeira, segunda) = bytes.split_at(32);
        assinatura.primeira.copy_from_slice(primeira);
        assinatura.segunda.copy_from_slice(segunda);
        assinatura
    }
    /// Os bytes que o verificador Ed25519 recebe.
    #[must_use]
    pub fn bytes(&self) -> [u8; 64] {
        let mut bytes = [0; 64];
        let (primeira, segunda) = bytes.split_at_mut(32);
        primeira.copy_from_slice(&self.primeira);
        segunda.copy_from_slice(&self.segunda);
        bytes
    }
}

/// Capacidade compartilhável: o id nasce aleatório, com 256 bits.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Convite {
    /// Identificador imprevisível. Não deve ir a logs nem listagens públicas.
    pub id: [u8; 32],
    /// Chave Ed25519 de quem recebe, que o link fixa antes de consultar o ponto.
    pub dono: [u8; 32],
    /// Último instante de admissão, em segundos Unix.
    pub expira_em: u64,
    /// Prova de posse da chave de quem recebe.
    pub assinatura: Assinatura,
}

/// Dados de apresentação do WebRTC; nenhum conteúdo de aplicação passa aqui.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub enum Sinal {
    /// Oferta local, produzida pelo navegador.
    Oferta {
        /// SDP completo, limitado.
        sdp: String,
        /// SHA-256 do certificado DTLS, conferido contra o SDP.
        impressao: [u8; 32],
    },
    /// Resposta local à oferta aceita.
    Resposta {
        /// SDP completo, limitado.
        sdp: String,
        /// SHA-256 do certificado DTLS, conferido contra o SDP.
        impressao: [u8; 32],
    },
    /// Um candidato adicional do agente ICE.
    Candidato {
        /// Linha produzida pelo navegador.
        candidato: String,
        /// Identificador da seção de mídia.
        mid: Option<String>,
        /// Índice da seção de mídia.
        linha: Option<u16>,
    },
    /// O agente não tem mais candidatos nesta geração.
    FimDosCandidatos,
}

/// Operações da versão 1. A ordem integra a canonização; não reordenar.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub enum Operacao {
    /// Registra/renova a caixa de quem recebe, sem estender o convite.
    Registrar {
        /// Convite original assinado por quem recebe.
        convite: Convite,
    },
    /// Pede o aceite; ainda não contém SDP, voz, tela ou texto de conversa.
    Chamar {
        /// Capacidade que a pessoa recebeu fora da ligação.
        convite: Convite,
        /// Nome declarado, limitado em UTF-8.
        apelido: String,
    },
    /// Leitura idempotente; confirmar nunca deve ultrapassar o cursor recebido.
    Buscar {
        /// Último evento que o cliente recebeu.
        cursor: u64,
        /// Último evento que o cliente terminou de consumir.
        confirmado: u64,
    },
    /// Quem recebe aceitou esta chamada e esta chave.
    Atender,
    /// Quem recebe recusou antes de abrir mídia.
    Recusar,
    /// Quem ligava desistiu antes de estabelecer a chamada.
    Cancelar,
    /// Fecha a chamada estabelecida; não revoga outros convites.
    Encerrar,
    /// Impede novas admissões; não derruba uma chamada já aceita.
    Revogar,
    /// Negociação de transporte, permitida só depois do aceite.
    Sinal(Sinal),
}

/// Pedido autenticado do endpoint `/chamada/1`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Papel {
    /// Pessoa que recebeu o link e pediu uma ligação.
    QuemLiga,
    /// Dona do convite e da caixa de recepção.
    QuemRecebe,
}

/// Pedido autenticado do endpoint `/chamada/1`.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Pedido {
    /// Papel explícito; assinatura de uma ponta não vale como a outra.
    pub papel: Papel,
    /// Época aleatória do processo do ponto; pedidos anteriores ao restart caem.
    pub epoca: [u8; 32],
    /// Nonce novo por requisição, inclusive polls. Repetir o mesmo é retry.
    pub nonce: [u8; 16],
    /// Capacidade do convite.
    pub convite: [u8; 32],
    /// Identificador aleatório da chamada; zero só em operações da caixa/link.
    pub chamada: [u8; 32],
    /// Chave Ed25519 que assina este pedido.
    pub remetente: [u8; 32],
    /// Chave que deve receber/interpretar o pedido.
    pub destinatario: [u8; 32],
    /// Geração da negociação, com proteção contra sinal antigo.
    pub geracao: u32,
    /// Sequência da mensagem nesta chamada/geração.
    pub sequencia: u64,
    /// Fim da validade deste pedido, separado da validade do convite.
    pub valido_ate: u64,
    /// A única carga encaminhada pelo ponto.
    pub operacao: Operacao,
    /// Assinatura dos bytes de [`mensagem_do_pedido`].
    pub assinatura: Assinatura,
}

/// Erros estruturais/temporais; não significam assinatura verificada.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Erro {
    /// Corpo/documento acima do teto.
    Grande,
    /// Forma ou contexto inconsistente.
    Invalido,
    /// Prazo já vencido.
    Expirado,
    /// Prazo futuro além do teto.
    Validade,
}

/// Razão pública de recusa do ponto, sem texto livre.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Motivo {
    /// Forma inválida.
    Invalido,
    /// Assinatura inválida ou chave fraca.
    Assinatura,
    /// Processo do ponto mudou; obtenha novo desafio antes de repetir.
    Epoca,
    /// Validade encerrada.
    Expirado,
    /// Convite revogado.
    Revogado,
    /// Quem recebe não mantém uma caixa ativa.
    SemRegistro,
    /// A chamada não existe.
    SemChamada,
    /// Não pertence à chamada/caixa ou fase não permite esta operação.
    SemPermissao,
    /// A chamada 1:1 já está ocupada.
    Ocupado,
    /// Nonce/sequência repetidos fora do retry permitido.
    Replay,
    /// Limite de memória atingido; não apaga tombstones para admitir outro.
    Cheio,
    /// Limite de requisições.
    Taxa,
}

/// Um evento conserva o pedido e a assinatura original do participante.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Evento {
    /// Posição na caixa; não é prova da identidade do pedido.
    pub cursor: u64,
    /// Envelope original. O cliente verifica sem confiar no ponto.
    pub pedido: Pedido,
}

/// Resposta do encontro. A caixa e o relógio não atestam a identidade do par.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub enum Resposta {
    /// Desafio público deste processo.
    Desafio {
        /// Época aleatória por processo.
        epoca: [u8; 32],
        /// Hora Unix do ponto.
        agora: u64,
    },
    /// Pedido aplicado.
    Ok,
    /// Leitura não destrutiva de caixa.
    Eventos {
        /// Pedidos originais em ordem de entrega.
        eventos: Vec<Evento>,
        /// Último cursor entregue nesta resposta.
        cursor: u64,
    },
    /// Pedido recusado sem texto de aplicação.
    Recusa {
        /// Razão estruturada.
        motivo: Motivo,
    },
}

fn validar_resposta(resposta: &Resposta) -> Result<(), Erro> {
    if let Resposta::Eventos { eventos, cursor } = resposta {
        if eventos.len() > 8 {
            return Err(Erro::Grande);
        }
        let mut antes = 0;
        for evento in eventos {
            if evento.cursor <= antes || evento.cursor > *cursor {
                return Err(Erro::Invalido);
            }
            validar_forma(&evento.pedido)?;
            antes = evento.cursor;
        }
    }
    Ok(())
}

/// Codifica resposta com limites de cardinalidade e bytes.
pub fn escrever_resposta(resposta: &Resposta) -> Result<String, Erro> {
    validar_resposta(resposta)?;
    let texto = serde_json::to_string(resposta).map_err(|_| Erro::Invalido)?;
    if texto.len() > MAX_RESPOSTA {
        return Err(Erro::Grande);
    }
    Ok(texto)
}

/// Decodifica resposta limitada. Os envelopes ainda precisam de autenticação.
pub fn ler_resposta(texto: &str) -> Result<Resposta, Erro> {
    if texto.len() > MAX_RESPOSTA {
        return Err(Erro::Grande);
    }
    let resposta = serde_json::from_str(texto).map_err(|_| Erro::Invalido)?;
    validar_resposta(&resposta)?;
    Ok(resposta)
}

/// Confere os limites temporais e estruturais de um convite, não sua assinatura.
pub fn validar_convite(convite: &Convite, agora: u64) -> Result<(), Erro> {
    if convite.id == [0; 32] || convite.dono == [0; 32] {
        return Err(Erro::Invalido);
    }
    validar_prazo(convite.expira_em, agora, VALIDADE_DO_CONVITE)
}

fn validar_prazo(ate: u64, agora: u64, teto: u64) -> Result<(), Erro> {
    if ate <= agora {
        return Err(Erro::Expirado);
    }
    if ate.saturating_sub(agora) > teto {
        return Err(Erro::Validade);
    }
    Ok(())
}

fn validar_forma(pedido: &Pedido) -> Result<(), Erro> {
    if pedido.epoca == [0; 32]
        || pedido.nonce == [0; 16]
        || pedido.convite == [0; 32]
        || pedido.remetente == [0; 32]
        || pedido.destinatario == [0; 32]
    {
        return Err(Erro::Invalido);
    }
    match pedido.operacao {
        Operacao::Registrar { .. } | Operacao::Revogar | Operacao::Atender | Operacao::Recusar
            if pedido.papel != Papel::QuemRecebe =>
        {
            return Err(Erro::Invalido);
        }
        Operacao::Chamar { .. } | Operacao::Cancelar if pedido.papel != Papel::QuemLiga => {
            return Err(Erro::Invalido);
        }
        _ => {}
    }
    match &pedido.operacao {
        Operacao::Registrar { convite } => {
            if pedido.convite != convite.id
                || pedido.remetente != convite.dono
                || pedido.destinatario != convite.dono
            {
                return Err(Erro::Invalido);
            }
        }
        Operacao::Chamar { convite, apelido } => {
            if pedido.convite != convite.id
                || pedido.destinatario != convite.dono
                || pedido.remetente == convite.dono
                || apelido.trim().is_empty()
                || apelido.len() > 128
                || apelido.chars().any(char::is_control)
            {
                return Err(Erro::Invalido);
            }
        }
        Operacao::Buscar { cursor, confirmado } if confirmado > cursor => {
            return Err(Erro::Invalido);
        }
        Operacao::Sinal(sinal) => validar_sinal(sinal)?,
        _ => {}
    }
    if !matches!(
        pedido.operacao,
        Operacao::Registrar { .. } | Operacao::Buscar { .. } | Operacao::Revogar
    ) && pedido.chamada == [0; 32]
    {
        return Err(Erro::Invalido);
    }
    Ok(())
}

fn validar_sinal(sinal: &Sinal) -> Result<(), Erro> {
    match sinal {
        Sinal::Oferta { sdp, impressao } | Sinal::Resposta { sdp, impressao } => {
            if sdp.len() > MAX_SDP {
                return Err(Erro::Grande);
            }
            if !sdp.starts_with("v=0\r\n") || sdp.contains('\0') {
                return Err(Erro::Invalido);
            }
            let esperada = impressao
                .iter()
                .map(|b| format!("{b:02X}"))
                .collect::<Vec<_>>()
                .join(":");
            let mut achou = false;
            for linha in sdp.lines() {
                if let Some(fp) = linha.strip_prefix("a=fingerprint:") {
                    let Some(hex) = fp.strip_prefix("sha-256 ") else {
                        return Err(Erro::Invalido);
                    };
                    if !hex.eq_ignore_ascii_case(&esperada) {
                        return Err(Erro::Invalido);
                    }
                    achou = true;
                }
            }
            if !achou {
                return Err(Erro::Invalido);
            }
        }
        Sinal::Candidato { candidato, mid, .. } => {
            if candidato.len() > MAX_CANDIDATO || mid.as_ref().is_some_and(|m| m.len() > 64) {
                return Err(Erro::Grande);
            }
            if !candidato.starts_with("candidate:")
                || candidato.chars().any(char::is_control)
                || mid
                    .as_ref()
                    .is_some_and(|m| m.chars().any(char::is_control))
            {
                return Err(Erro::Invalido);
            }
        }
        Sinal::FimDosCandidatos => {}
    }
    Ok(())
}

/// Confere forma/tempo. O consumidor ainda DEVE verificar as assinaturas.
pub fn validar_pedido(pedido: &Pedido, agora: u64) -> Result<(), Erro> {
    validar_forma(pedido)?;
    validar_prazo(pedido.valido_ate, agora, VALIDADE_DO_PEDIDO)?;
    if let Operacao::Registrar { convite } | Operacao::Chamar { convite, .. } = &pedido.operacao {
        validar_convite(convite, agora)?;
    }
    Ok(())
}

/// Bytes canônicos do convite, sob domínio próprio. Não contém a assinatura.
#[must_use]
pub fn mensagem_do_convite(convite: &Convite) -> Vec<u8> {
    let mut bytes = crate::assinatura::ROTULO_CONVITE_PESSOAL.to_vec();
    bytes.extend_from_slice(&convite.id);
    bytes.extend_from_slice(&convite.dono);
    bytes.extend_from_slice(&convite.expira_em.to_be_bytes());
    bytes
}

/// Bytes canônicos do pedido. Tipo/carga entram pelo digest de sua codificação
/// Rust; mudar SDP, candidato, aceite, cursor ou assinatura do convite muda a prova.
pub fn mensagem_do_pedido(pedido: &Pedido) -> Result<Vec<u8>, Erro> {
    validar_forma(pedido)?;
    let corpo = postcard::to_allocvec(&pedido.operacao).map_err(|_| Erro::Invalido)?;
    if corpo.len() > MAX_PEDIDO {
        return Err(Erro::Grande);
    }
    let mut bytes = crate::assinatura::ROTULO_PEDIDO_PESSOAL.to_vec();
    bytes.push(match pedido.papel {
        Papel::QuemLiga => 0,
        Papel::QuemRecebe => 1,
    });
    bytes.extend_from_slice(&pedido.epoca);
    bytes.extend_from_slice(&pedido.nonce);
    bytes.extend_from_slice(&pedido.convite);
    bytes.extend_from_slice(&pedido.chamada);
    bytes.extend_from_slice(&pedido.remetente);
    bytes.extend_from_slice(&pedido.destinatario);
    bytes.extend_from_slice(&pedido.geracao.to_be_bytes());
    bytes.extend_from_slice(&pedido.sequencia.to_be_bytes());
    bytes.extend_from_slice(&pedido.valido_ate.to_be_bytes());
    bytes.extend_from_slice(&(corpo.len() as u64).to_be_bytes());
    bytes.extend_from_slice(&Sha256::digest(&corpo));
    Ok(bytes)
}

/// Decodifica JSON limitado, recusando campos desconhecidos e forma inválida.
pub fn ler_pedido(texto: &str) -> Result<Pedido, Erro> {
    if texto.len() > MAX_PEDIDO {
        return Err(Erro::Grande);
    }
    let pedido = serde_json::from_str(texto).map_err(|_| Erro::Invalido)?;
    validar_forma(&pedido)?;
    Ok(pedido)
}

/// Codifica pelo contrato Rust; a página não escreve o protocolo.
pub fn escrever_pedido(pedido: &Pedido) -> Result<String, Erro> {
    validar_forma(pedido)?;
    let texto = serde_json::to_string(pedido).map_err(|_| Erro::Invalido)?;
    if texto.len() > MAX_PEDIDO {
        return Err(Erro::Grande);
    }
    Ok(texto)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn pedido() -> Pedido {
        Pedido {
            papel: Papel::QuemLiga,
            epoca: [1; 32],
            nonce: [2; 16],
            convite: [3; 32],
            chamada: [4; 32],
            remetente: [5; 32],
            destinatario: [6; 32],
            geracao: 1,
            sequencia: 2,
            valido_ate: 150,
            operacao: Operacao::Sinal(Sinal::FimDosCandidatos),
            assinatura: Assinatura::default(),
        }
    }

    #[test]
    fn cada_contexto_assinado_muda_a_prova() {
        let original = pedido();
        let bytes = mensagem_do_pedido(&original).unwrap();
        let mut alterados = Vec::new();
        macro_rules! alterar {
            ($campo:ident, $valor:expr) => {{
                let mut p = original.clone();
                p.$campo = $valor;
                alterados.push(p);
            }};
        }
        alterar!(papel, Papel::QuemRecebe);
        alterar!(epoca, [2; 32]);
        alterar!(nonce, [3; 16]);
        alterar!(convite, [4; 32]);
        alterar!(chamada, [5; 32]);
        alterar!(remetente, [7; 32]);
        alterar!(destinatario, [8; 32]);
        alterar!(geracao, 2);
        alterar!(sequencia, 3);
        alterar!(valido_ate, 151);
        alterar!(operacao, Operacao::Encerrar);
        for outro in alterados {
            assert_ne!(mensagem_do_pedido(&outro).unwrap(), bytes);
        }
        let mut assinado = original;
        assinado.assinatura = Assinatura::de_bytes([9; 64]);
        assert_eq!(mensagem_do_pedido(&assinado).unwrap(), bytes);
        assert_eq!(assinado.assinatura.bytes(), [9; 64]);
    }

    #[test]
    fn sdp_inteiro_e_impressao_sao_vinculados() {
        let fp = [7; 32];
        let hex = fp
            .iter()
            .map(|b| format!("{b:02X}"))
            .collect::<Vec<_>>()
            .join(":");
        let mut a = pedido();
        a.operacao = Operacao::Sinal(Sinal::Oferta {
            sdp: format!("v=0\r\na=fingerprint:sha-256 {hex}\r\na=ice-ufrag:um\r\n"),
            impressao: fp,
        });
        let mut b = a.clone();
        if let Operacao::Sinal(Sinal::Oferta { sdp, .. }) = &mut b.operacao {
            *sdp = sdp.replace("ufrag:um", "ufrag:outro");
        }
        assert_ne!(
            mensagem_do_pedido(&a).unwrap(),
            mensagem_do_pedido(&b).unwrap()
        );
        if let Operacao::Sinal(Sinal::Oferta { impressao, .. }) = &mut b.operacao {
            *impressao = [8; 32];
        }
        assert_eq!(mensagem_do_pedido(&b), Err(Erro::Invalido));
    }

    #[test]
    fn limites_tempo_e_codec_nao_aceitam_documento_arbitrario() {
        let p = pedido();
        assert_eq!(validar_pedido(&p, 100), Ok(()));
        assert_eq!(validar_pedido(&p, 150), Err(Erro::Expirado));
        assert_eq!(validar_pedido(&p, 89), Err(Erro::Validade));
        let texto = escrever_pedido(&p).unwrap();
        assert_eq!(ler_pedido(&texto).unwrap(), p);
        let estranho = texto.replacen('{', "{\"desconhecido\":true,", 1);
        assert_eq!(ler_pedido(&estranho), Err(Erro::Invalido));
        assert_eq!(ler_pedido(&" ".repeat(MAX_PEDIDO + 1)), Err(Erro::Grande));
        let mut b = p;
        b.operacao = Operacao::Buscar {
            cursor: 1,
            confirmado: 2,
        };
        assert_eq!(mensagem_do_pedido(&b), Err(Erro::Invalido));
    }

    #[test]
    fn convite_tem_dominio_e_validade_proprios() {
        let a = Convite {
            id: [3; 32],
            dono: [6; 32],
            expira_em: 1000,
            assinatura: Assinatura::default(),
        };
        assert_eq!(validar_convite(&a, 100), Ok(()));
        assert_eq!(validar_convite(&a, 1000), Err(Erro::Expirado));
        let mut b = a.clone();
        b.expira_em += 1;
        assert_ne!(mensagem_do_convite(&a), mensagem_do_convite(&b));
        assert!(mensagem_do_convite(&a).starts_with(crate::assinatura::ROTULO_CONVITE_PESSOAL));
    }
}

/// Controle direto da conversa, nunca encaminhado pelo ponto de encontro.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub enum Controle {
    /// Mensagem UTF-8, limitada antes de alocar o documento.
    Texto {
        /// Identificador monotônico no canal desta ponta.
        id: u64,
        /// Conteúdo limitado a oito mil bytes UTF-8.
        texto: String,
    },
    /// Retrato da transmissão; não inicializa o decodificador de vídeo.
    TelaComecou {
        /// Largura informativa em pixels.
        largura: u32,
        /// Altura informativa em pixels.
        altura: u32,
    },
    /// A transmissão foi interrompida.
    TelaParou,
    /// O receptor precisa de um quadro independente.
    PedirChave,
}

fn controle_valido(controle: &Controle) -> bool {
    match controle {
        Controle::Texto { id, texto } => *id > 0 && !texto.trim().is_empty() && texto.len() <= 8000,
        Controle::TelaComecou { largura, altura } => {
            (1..=16384).contains(largura) && (1..=16384).contains(altura)
        }
        Controle::TelaParou | Controle::PedirChave => true,
    }
}

/// Serializa apenas o vocabulário do canal direto.
pub fn escrever_controle(controle: &Controle) -> Result<Vec<u8>, Erro> {
    if !controle_valido(controle) {
        return Err(Erro::Invalido);
    }
    postcard::to_allocvec(controle).map_err(|_| Erro::Invalido)
}

/// Decodifica sem aceitar bytes sobrando nem texto ilimitado.
pub fn ler_controle(bytes: &[u8]) -> Result<Controle, Erro> {
    if bytes.len() > 8192 {
        return Err(Erro::Grande);
    }
    let (controle, sobra): (Controle, _) =
        postcard::take_from_bytes(bytes).map_err(|_| Erro::Invalido)?;
    if !sobra.is_empty() || !controle_valido(&controle) {
        return Err(Erro::Invalido);
    }
    Ok(controle)
}

/// O canal ordenado da tela delimita suas próprias gerações. Controle e tela
/// não dependem da ordem relativa de entrega de dois DataChannels distintos.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub enum TelaPessoal {
    /// Começa uma nova transmissão, com geração estritamente crescente.
    Abrir {
        /// Número positivo que distingue transmissões sucessivas.
        geracao: u32,
    },
    /// Pedaço do fluxo SEELE existente (abertura, cabeçalhos e vídeo).
    Dados {
        /// Transmissão à qual pertencem os bytes.
        geracao: u32,
        /// Pedaço limitado a dezesseis mil bytes.
        bytes: Vec<u8>,
    },
    /// Fecha apenas a geração indicada.
    Fechar {
        /// Transmissão que termina, sem afetar uma mais nova.
        geracao: u32,
    },
}
fn tela_valida(mensagem: &TelaPessoal) -> bool {
    match mensagem {
        TelaPessoal::Abrir { geracao } | TelaPessoal::Fechar { geracao } => *geracao > 0,
        TelaPessoal::Dados { geracao, bytes } => {
            *geracao > 0 && !bytes.is_empty() && bytes.len() <= 16000
        }
    }
}
/// Delimita um bloco de vídeo para o canal direto.
pub fn escrever_tela(mensagem: &TelaPessoal) -> Result<Vec<u8>, Erro> {
    if !tela_valida(mensagem) {
        return Err(Erro::Invalido);
    }
    postcard::to_allocvec(mensagem).map_err(|_| Erro::Invalido)
}
/// Recusa envelope grande ou uma cauda não consumida.
pub fn ler_tela(bytes: &[u8]) -> Result<TelaPessoal, Erro> {
    if bytes.len() > 16384 {
        return Err(Erro::Grande);
    }
    let (mensagem, sobra): (TelaPessoal, _) =
        postcard::take_from_bytes(bytes).map_err(|_| Erro::Invalido)?;
    if !sobra.is_empty() || !tela_valida(&mensagem) {
        return Err(Erro::Invalido);
    }
    Ok(mensagem)
}

/// Representação canônica e compacta que fica SOMENTE no fragmento do link.
pub fn escrever_convite(convite: &Convite) -> Result<String, Erro> {
    use base64::{engine::general_purpose::URL_SAFE_NO_PAD, Engine};
    let bytes = postcard::to_allocvec(convite).map_err(|_| Erro::Invalido)?;
    Ok(URL_SAFE_NO_PAD.encode(bytes))
}
/// Lê o fragmento; quem chama ainda deve verificar assinatura e prazo.
pub fn ler_convite(fragmento: &str) -> Result<Convite, Erro> {
    use base64::{engine::general_purpose::URL_SAFE_NO_PAD, Engine};
    if fragmento.len() > 256 {
        return Err(Erro::Grande);
    }
    let bytes = URL_SAFE_NO_PAD
        .decode(fragmento)
        .map_err(|_| Erro::Invalido)?;
    let (convite, sobra) = postcard::take_from_bytes(&bytes).map_err(|_| Erro::Invalido)?;
    if !sobra.is_empty() || escrever_convite(&convite)? != fragmento {
        return Err(Erro::Invalido);
    }
    Ok(convite)
}

#[cfg(test)]
#[allow(clippy::unwrap_used, reason = "vetores sintéticos de teste")]
mod codecs_pessoais_tests {
    use super::*;
    #[test]
    fn codecs_limitam_e_nao_aceitam_cauda() {
        let texto = Controle::Texto {
            id: 1,
            texto: "Olá".into(),
        };
        let mut bytes = escrever_controle(&texto).unwrap();
        assert_eq!(ler_controle(&bytes).unwrap(), texto);
        bytes.push(0);
        assert!(ler_controle(&bytes).is_err());
        assert!(escrever_controle(&Controle::Texto {
            id: 0,
            texto: "a".into()
        })
        .is_err());
        assert!(escrever_controle(&Controle::Texto {
            id: 1,
            texto: "a".repeat(8001)
        })
        .is_err());
        let tela = TelaPessoal::Dados {
            geracao: 1,
            bytes: vec![7; 16000],
        };
        let bytes = escrever_tela(&tela).unwrap();
        assert!(bytes.len() <= 16384);
        assert_eq!(ler_tela(&bytes).unwrap(), tela);
        assert!(escrever_tela(&TelaPessoal::Abrir { geracao: 0 }).is_err());
        assert!(escrever_tela(&TelaPessoal::Dados {
            geracao: 1,
            bytes: vec![0; 16001]
        })
        .is_err());
    }
    #[test]
    fn convite_so_tem_uma_escrita() {
        let convite = Convite {
            id: [1; 32],
            dono: [2; 32],
            expira_em: 123456,
            assinatura: Assinatura::default(),
        };
        let texto = escrever_convite(&convite).unwrap();
        assert_eq!(ler_convite(&texto).unwrap(), convite);
        assert!(ler_convite(&(texto + "=")).is_err());
    }
}
