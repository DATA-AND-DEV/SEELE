//! As esperas de volume: quem autorizou o quê, para quem, e até quando.
//!
//! ADR 0048. Os bytes de uma imagem deixam de atravessar o canal de controle e
//! passam por um fluxo próprio, direto ao disco — o QuickJS nunca os vê. Isso
//! abre uma pergunta que o caminho antigo não tinha: **contra o que o servidor
//! confere um fluxo que chega?**
//!
//! O token nasce dentro do MOD, e o servidor não interpreta o estado do MOD nem
//! deve. Se o cabeçalho do fluxo trouxesse só `{mod, token}`, «autorizar» seria
//! formalidade: bastaria abrir um fluxo com qualquer token inventado.
//!
//! Então o MOD **registra a espera** aqui, de dentro do `aoPedir`, no mesmo ato
//! em que responde à janela. É contra ela que o cabeçalho é conferido.
//!
//! # Três propriedades, e cada uma fecha um buraco
//!
//! - **A pessoa vem do servidor, não do MOD.** `esperar` não recebe quem é: ele
//!   lê de quem o `Hospedes::pedir` disse estar atendendo. Um MOD com defeito
//!   não tem como prender um token à pessoa errada, e um MOD mal-intencionado
//!   não tem como prendê-lo a alguém que não pediu nada.
//! - **Vale uma vez.** [`Esperas::tomar`] remove. Sem isso, um token vazado
//!   viraria um lugar de escrita permanente na pasta do MOD, para quem o
//!   tivesse.
//! - **Tem prazo.** Uma espera que ninguém consome não fica de pé para sempre
//!   ocupando memória e esperando alguém que desistiu.

#![cfg_attr(test, allow(clippy::unwrap_used, clippy::expect_used))]

use std::collections::HashMap;
use std::time::{Duration, Instant};

use seele_proto::ids::PersonId;

/// A pasta, dentro dos dados de um MOD, onde o volume mora.
///
/// **Nomeada à parte de propósito** — ADR 0048. Um arquivo de volume não é um
/// arquivo de MOD comum: `arquivos::escrever` tem teto de 4 MiB por arquivo, e
/// o volume não tem teto nenhum. Misturá-los apagaria aquele teto para todo o
/// resto, que é o que protege o disco de quem hospeda hoje.
pub const PASTA: &str = "volume";

/// Quanto entra e sai do disco de uma vez.
///
/// O mesmo de `transfer.rs`, e pela mesma razão escrita lá: o servidor é
/// dimensionado em 1 vCPU e 512 MB, e nada segura o arquivo inteiro.
pub const BLOCO: usize = 64 * 1024;

/// O maior prazo que uma espera pode pedir.
///
/// Dez minutos é o que o envio em fragmentos já usava, e sobra: por fluxo, uma
/// imagem de 10 MiB passa dentro da rajada de bytes. O teto existe para que um
/// MOD não peça um prazo de um dia e deixe esperas de pé até o servidor
/// reiniciar.
pub const PRAZO_MAXIMO: Duration = Duration::from_secs(600);

/// Quantas esperas de pé ao mesmo tempo, no servidor inteiro.
///
/// **Isto não é o teto de disco**, que o ADR 0048 decidiu não existir. É o teto
/// da *lista*: uma espera custa memória antes de qualquer byte chegar, e um MOD
/// num laço registrando esperas encheria a memória de quem hospeda sem nunca
/// enviar nada. O disco é decisão de quem hospeda; a memória do processo não.
pub const ESPERAS_DE_PE: usize = 256;

/// Uma autorização de escrita, registrada pelo MOD e conferida pelo servidor.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Espera {
    /// Qual MOD autorizou. O fluxo tem de nomear o mesmo.
    pub mod_id: String,
    /// Para quem. Vem do servidor, e não do MOD.
    pub pessoa: PersonId,
    /// Onde gravar, dentro da pasta daquele MOD. Passa pelo `inner_path` de
    /// sempre na hora de resolver — aqui é guardado como o MOD o escreveu.
    pub caminho: String,
    /// Os tipos de imagem que este MOD aceita neste envio.
    ///
    /// É o servidor quem confere os bytes (ADR 0048), e é esta lista que diz
    /// contra o quê. Vazia significa «nenhum», e não «todos»: um MOD que
    /// esquecer de declarar não passa a aceitar qualquer coisa.
    pub tipos: Vec<String>,
    /// Até quando ela vale.
    pub ate: Instant,
}

/// As esperas de pé.
#[derive(Debug, Default)]
pub struct Esperas {
    por_token: HashMap<String, Espera>,
}

/// Por que uma espera não foi registrada.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NaoEsperou {
    /// Chamado fora de um pedido: não há quem atender.
    SemPedidoEmCurso,
    /// Token vazio, caminho vazio, ou nenhum tipo declarado.
    PedidoIncompleto,
    /// Já há uma espera com este token.
    TokenRepetido,
    /// A lista está cheia. Ver [`ESPERAS_DE_PE`].
    ListaCheia,
}

/// O que o MOD pede ao registrar uma espera.
///
/// Uma estrutura e não sete argumentos: os campos são todos texto e número, e
/// uma assinatura assim é uma assinatura em que trocar dois de lugar compila.
pub struct PedidoDeEspera {
    /// Qual MOD autoriza. Posto pelo servidor, não pelo MOD.
    pub mod_id: String,
    /// Para quem. Posto pelo servidor, não pelo MOD.
    pub pessoa: PersonId,
    /// O token que o fluxo vai apresentar.
    pub token: String,
    /// Onde gravar, dentro da pasta daquele MOD.
    pub caminho: String,
    /// Os tipos de imagem aceitos. Vazia é «nenhum», e não «todos».
    pub tipos: Vec<String>,
    /// Quanto tempo a espera vale, aparado em [`PRAZO_MAXIMO`].
    pub prazo: Duration,
}

impl Esperas {
    /// Regista uma espera, ou diz por que não.
    ///
    /// O prazo é aparado em [`PRAZO_MAXIMO`]; um MOD que peça mais recebe o
    /// máximo em vez de uma recusa, porque pedir demais é engano comum e
    /// recusar por isso não protege ninguém.
    ///
    /// # Errors
    ///
    /// [`NaoEsperou`] quando falta o que identifica a espera, quando o token
    /// repete, ou quando a lista está cheia.
    pub fn registrar(&mut self, pedido: PedidoDeEspera, agora: Instant) -> Result<(), NaoEsperou> {
        if pedido.token.is_empty() || pedido.caminho.is_empty() || pedido.tipos.is_empty() {
            return Err(NaoEsperou::PedidoIncompleto);
        }
        self.podar(agora);
        if self.por_token.contains_key(&pedido.token) {
            return Err(NaoEsperou::TokenRepetido);
        }
        if self.por_token.len() >= ESPERAS_DE_PE {
            return Err(NaoEsperou::ListaCheia);
        }
        self.por_token.insert(
            pedido.token,
            Espera {
                mod_id: pedido.mod_id,
                pessoa: pedido.pessoa,
                caminho: pedido.caminho,
                tipos: pedido.tipos,
                ate: agora + pedido.prazo.min(PRAZO_MAXIMO),
            },
        );
        Ok(())
    }

    /// Consome a espera deste token, se ela é desta pessoa, deste MOD, e ainda
    /// vale.
    ///
    /// **Remove.** Uma espera consumida não existe mais: é o que impede um
    /// token vazado de virar escrita repetida.
    pub fn tomar(
        &mut self,
        token: &str,
        mod_id: &str,
        pessoa: PersonId,
        agora: Instant,
    ) -> Option<Espera> {
        let espera = self.por_token.get(token)?;
        // Conferir **antes** de remover: um token certo com a pessoa errada não
        // pode consumir a espera de quem a registrou. Seria transformar um
        // palpite numa negação de serviço.
        if espera.mod_id != mod_id || espera.pessoa != pessoa || espera.ate <= agora {
            return None;
        }
        self.por_token.remove(token)
    }

    /// Esquece o que venceu.
    pub fn podar(&mut self, agora: Instant) {
        self.por_token.retain(|_, e| e.ate > agora);
    }

    /// Quantas estão de pé. Para o teste e para quem hospeda olhar.
    #[must_use]
    pub fn quantas(&self) -> usize {
        self.por_token.len()
    }
}

#[cfg(test)]
mod testes {
    use super::*;

    fn pessoa(n: u64) -> PersonId {
        PersonId(n)
    }

    fn pedido(token: &str, quem: PersonId, tipos: Vec<String>, prazo: u64) -> PedidoDeEspera {
        PedidoDeEspera {
            mod_id: "seele/perfis".to_owned(),
            pessoa: quem,
            token: token.to_owned(),
            caminho: "volume/avatar.bin".to_owned(),
            tipos,
            prazo: Duration::from_secs(prazo),
        }
    }

    fn registrada(esperas: &mut Esperas, token: &str, quem: PersonId, agora: Instant) {
        esperas
            .registrar(pedido(token, quem, vec!["png".to_owned()], 60), agora)
            .expect("registrar");
    }

    #[test]
    fn a_espera_registrada_e_consumida_uma_vez_so() {
        let agora = Instant::now();
        let mut esperas = Esperas::default();
        registrada(&mut esperas, "t1", pessoa(7), agora);

        assert!(esperas
            .tomar("t1", "seele/perfis", pessoa(7), agora)
            .is_some());
        assert!(
            esperas
                .tomar("t1", "seele/perfis", pessoa(7), agora)
                .is_none(),
            "um token vazado viraria um lugar de escrita permanente na pasta do MOD"
        );
    }

    #[test]
    fn a_espera_e_de_uma_pessoa_e_de_um_mod() {
        let agora = Instant::now();
        let mut esperas = Esperas::default();
        registrada(&mut esperas, "t1", pessoa(7), agora);

        assert!(
            esperas
                .tomar("t1", "seele/perfis", pessoa(8), agora)
                .is_none(),
            "outra pessoa consumiu a autorização de quem a pediu"
        );
        assert!(
            esperas
                .tomar("t1", "seele/estilo", pessoa(7), agora)
                .is_none(),
            "outro MOD escreveu na pasta usando token alheio"
        );
        assert_eq!(
            esperas.quantas(),
            1,
            "a tentativa errada consumiu a espera de quem tinha direito a ela: \
             adivinhar um token viraria negação de serviço"
        );
        assert!(esperas
            .tomar("t1", "seele/perfis", pessoa(7), agora)
            .is_some());
    }

    #[test]
    fn uma_espera_vencida_nao_vale_e_nao_fica_de_pe() {
        let agora = Instant::now();
        let mut esperas = Esperas::default();
        registrada(&mut esperas, "t1", pessoa(7), agora);

        let depois = agora + Duration::from_secs(61);
        assert!(esperas
            .tomar("t1", "seele/perfis", pessoa(7), depois)
            .is_none());
        esperas.podar(depois);
        assert_eq!(esperas.quantas(), 0);
    }

    /// Um MOD que peça um dia recebe o máximo, e não uma recusa: pedir demais é
    /// engano comum, e recusar por isso não protege ninguém.
    #[test]
    fn um_prazo_alem_do_teto_e_aparado_e_nao_recusado() {
        let agora = Instant::now();
        let mut esperas = Esperas::default();
        esperas
            .registrar(
                pedido("t1", pessoa(7), vec!["png".to_owned()], 86_400),
                agora,
            )
            .expect("registrar");

        let quase = agora + PRAZO_MAXIMO - Duration::from_secs(1);
        assert!(esperas
            .tomar("t1", "seele/perfis", pessoa(7), quase)
            .is_some());
    }

    /// Vazia é **nenhum**, e não «todos»: um MOD que esquecer de declarar os
    /// tipos não passa a aceitar qualquer coisa.
    #[test]
    fn uma_espera_sem_tipo_declarado_nao_e_registrada() {
        let agora = Instant::now();
        let mut esperas = Esperas::default();
        assert_eq!(
            esperas.registrar(pedido("t1", pessoa(7), Vec::new(), 60), agora),
            Err(NaoEsperou::PedidoIncompleto)
        );
        assert_eq!(esperas.quantas(), 0);
    }

    /// O teto é da **lista**, e não do disco: uma espera custa memória antes de
    /// um byte chegar, e um MOD num laço encheria a memória de quem hospeda.
    #[test]
    fn um_laco_de_esperas_nao_enche_a_memoria_de_quem_hospeda() {
        let agora = Instant::now();
        let mut esperas = Esperas::default();
        for n in 0..ESPERAS_DE_PE {
            registrada(&mut esperas, &format!("t{n}"), pessoa(7), agora);
        }
        assert_eq!(
            esperas.registrar(
                pedido("demais", pessoa(7), vec!["png".to_owned()], 60),
                agora
            ),
            Err(NaoEsperou::ListaCheia)
        );
        assert_eq!(esperas.quantas(), ESPERAS_DE_PE);
    }

    /// E a lista cheia de esperas **vencidas** não tranca o servidor: a poda
    /// acontece antes de recusar, senão um pico de dez minutos atrás impediria
    /// todo envio até alguém reiniciar.
    #[test]
    fn a_lista_cheia_de_vencidas_volta_a_aceitar_sozinha() {
        let agora = Instant::now();
        let mut esperas = Esperas::default();
        for n in 0..ESPERAS_DE_PE {
            registrada(&mut esperas, &format!("t{n}"), pessoa(7), agora);
        }
        let depois = agora + Duration::from_secs(61);
        esperas
            .registrar(
                pedido("nova", pessoa(7), vec!["png".to_owned()], 60),
                depois,
            )
            .expect("a poda tem de abrir espaço");
        assert_eq!(esperas.quantas(), 1);
    }

    #[test]
    fn o_mesmo_token_duas_vezes_e_recusado() {
        let agora = Instant::now();
        let mut esperas = Esperas::default();
        registrada(&mut esperas, "t1", pessoa(7), agora);
        assert_eq!(
            esperas.registrar(pedido("t1", pessoa(7), vec!["png".to_owned()], 60), agora),
            Err(NaoEsperou::TokenRepetido)
        );
    }
}

// ---------------------------------------------------------------- o recebimento

/// O que um fluxo de volume deixou no disco.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Chegou {
    /// De qual MOD é a pasta.
    pub mod_id: String,
    /// Para quem a espera estava registrada.
    pub pessoa: PersonId,
    /// A autorização consumida. Vai de volta ao MOD no veredito.
    pub token: String,
    /// Onde o arquivo ficou, como o MOD o nomeou.
    pub caminho: String,
    /// Quantos bytes chegaram.
    pub bytes: u64,
    /// O SHA-256 do conteúdo, em hexadecimal minúsculo.
    pub hash: String,
    /// O tipo que os **bytes** dizem ser — nunca o que alguém declarou.
    pub tipo: &'static str,
}

/// Recebe um fluxo de volume, do cabeçalho ao arquivo em disco.
///
/// # A ordem das conferências, e por que ela é esta
///
/// 1. **O cabeçalho cabe no fio.** Antes de qualquer alocação.
/// 2. **A espera existe, é deste MOD, desta pessoa, e ainda vale.** Consumida
///    aqui, e por isso um token vazado não vira escrita repetida.
/// 3. **O caminho resolve dentro da pasta.** Pelo `dentro` de sempre, e sob
///    [`PASTA`]: um caminho que sobe morre aqui, mesmo tendo vindo do MOD.
/// 4. **Os bytes vão para um arquivo de obras**, em blocos, com o hash sendo
///    somado no caminho. Nada segura o arquivo inteiro.
/// 5. **O tipo é lido dos bytes** e conferido contra o que o MOD declarou
///    aceitar. Só então o arquivo de obras vira o arquivo final.
///
/// O quinto passo é o que torna «sem teto» uma decisão e não um descuido do
/// outro lado: o arquivo só aparece no lugar que o MOD nomeou depois de o
/// produto ter olhado os bytes.
///
/// # Errors
///
/// Devolve [`seele_proto::volume::VolumeRefusal`] — uma recusa é algo que quem
/// enviou está esperando ouvir, e não um erro de quem recebe.
pub async fn receber(
    esperas: &std::sync::Mutex<Esperas>,
    raizes: Option<&crate::RaizesDosMods>,
    pessoa: PersonId,
    fluxo: &mut quinn::RecvStream,
) -> Result<Chegou, (String, seele_proto::volume::VolumeRefusal)> {
    use seele_proto::volume::VolumeRefusal;

    let cabecalho: seele_proto::volume::VolumeHeader = crate::frame::read(fluxo)
        .await
        .map_err(|_| (String::new(), VolumeRefusal::Incompleto))?;
    {
        use seele_proto::control::Validate as _;
        if cabecalho.validate().is_err() {
            return Err((String::new(), VolumeRefusal::SemEspera));
        }
    }
    let token = cabecalho.token.clone();
    let errar = |motivo| (token.clone(), motivo);

    let espera = {
        let Ok(mut guarda) = esperas.lock() else {
            return Err(errar(VolumeRefusal::NaoGravei));
        };
        guarda
            .tomar(&cabecalho.token, &cabecalho.mod_id, pessoa, Instant::now())
            .ok_or_else(|| errar(VolumeRefusal::SemEspera))?
    };

    // Sem raiz não há onde gravar: este servidor é o de memória, e a recusa é
    // do disco e não do token — quem enviou precisa saber a diferença.
    let Some(raizes) = raizes else {
        return Err(errar(VolumeRefusal::NaoGravei));
    };
    // **Da instância, e não da máquina.** Ver `RaizesDosMods`: o mesmo MOD em
    // dois servidores desta máquina grava em lugares diferentes.
    let raiz = raizes.dados_de(&espera.mod_id).join(PASTA);
    let Some(destino) = crate::mods::arquivos::dentro(&raiz, &espera.caminho) else {
        return Err(errar(VolumeRefusal::SemEspera));
    };
    let Some(pai) = destino.parent() else {
        return Err(errar(VolumeRefusal::NaoGravei));
    };
    if tokio::fs::create_dir_all(pai).await.is_err() {
        return Err(errar(VolumeRefusal::NaoGravei));
    }

    // **Arquivo de obras, e não o destino.** Um fluxo que cai no meio deixaria
    // meia imagem no lugar onde o MOD espera uma imagem inteira — e o MOD não
    // tem como saber disso sem abrir os bytes, que é exatamente o que este
    // caminho existe para ele não precisar fazer.
    let obras = pai.join(format!(".em-obras-{}", cabecalho.token));
    let resultado = escorrer(fluxo, &obras, &espera).await;
    match resultado {
        Ok(chegou) => {
            if tokio::fs::rename(&obras, &destino).await.is_err() {
                let _ = tokio::fs::remove_file(&obras).await;
                return Err(errar(VolumeRefusal::NaoGravei));
            }
            Ok(Chegou {
                mod_id: espera.mod_id,
                pessoa: espera.pessoa,
                token: cabecalho.token,
                caminho: espera.caminho,
                ..chegou
            })
        }
        Err(motivo) => {
            // O de obras sai em toda saída que não é a de cima. Deixá-lo seria
            // lixo com nome de token na pasta de quem hospeda.
            let _ = tokio::fs::remove_file(&obras).await;
            Err(errar(motivo))
        }
    }
}

/// Os bytes, em blocos, com o hash somado no caminho.
///
/// Devolve um [`Chegou`] com os campos que só os bytes respondem; quem chama
/// preenche o resto.
async fn escorrer(
    fluxo: &mut quinn::RecvStream,
    obras: &std::path::Path,
    espera: &Espera,
) -> Result<Chegou, seele_proto::volume::VolumeRefusal> {
    use seele_proto::attachment::ContentDigest;
    use seele_proto::volume::VolumeRefusal;
    use tokio::io::AsyncWriteExt;

    let mut arquivo = tokio::fs::File::create(obras)
        .await
        .map_err(|_| VolumeRefusal::NaoGravei)?;
    // O somador do anexo, e não um segundo: o hash de um arquivo neste produto
    // é o mesmo hash, somado pelo mesmo código.
    let mut soma = ContentDigest::new();
    let mut bytes = 0_u64;
    // Os primeiros bytes ficam guardados: é neles que o tipo é lido, e ler o
    // tipo do arquivo depois de escrevê-lo seria ler de novo o que já passou.
    let mut comeco: Vec<u8> = Vec::with_capacity(16);
    let mut bloco = vec![0_u8; BLOCO];

    loop {
        let lido = fluxo
            .read(&mut bloco)
            .await
            .map_err(|_| VolumeRefusal::Incompleto)?;
        let Some(quantos) = lido else { break };
        let pedaco = bloco.get(..quantos).unwrap_or_default();
        if comeco.len() < seele_proto::imagem::SNIFF_LEN {
            let falta = seele_proto::imagem::SNIFF_LEN - comeco.len();
            comeco.extend_from_slice(pedaco.get(..falta).unwrap_or(pedaco));
        }
        soma.feed(pedaco);
        bytes = bytes.saturating_add(quantos as u64);
        arquivo
            .write_all(pedaco)
            .await
            .map_err(|_| VolumeRefusal::NaoGravei)?;
    }
    arquivo
        .flush()
        .await
        .map_err(|_| VolumeRefusal::NaoGravei)?;
    drop(arquivo);

    // **O tipo vem dos bytes**, com a mesma tabela de quatro formatos que a
    // prévia de anexo usa — reúso, e não uma segunda cópia da regra. Decisão 2
    // do ADR 0048: o MOD não tem como conferir o que ele não vê.
    let formato = seele_proto::imagem::sniff(&comeco).ok_or(VolumeRefusal::TipoRecusado)?;
    // O MOD declara os tipos como `png`, `jpeg`, `webp`, `gif` — o sufixo do
    // tipo de mídia, que é como o `data:` da janela já os escrevia.
    if !espera.tipos.iter().any(|t| t == formato.sufixo()) {
        return Err(VolumeRefusal::TipoRecusado);
    }

    Ok(Chegou {
        mod_id: String::new(),
        pessoa: espera.pessoa,
        token: String::new(),
        caminho: String::new(),
        bytes,
        hash: hex(&soma.finish()),
        tipo: formato.media_type(),
    })
}

/// Hexadecimal minúsculo, como todo hash que uma pessoa compara a olho.
fn hex(bytes: &[u8]) -> String {
    use std::fmt::Write as _;
    bytes.iter().fold(String::new(), |mut saida, byte| {
        let _ = write!(saida, "{byte:02x}");
        saida
    })
}
