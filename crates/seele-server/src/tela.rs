//! MEDIA encaminha a tela, como já encaminha a voz.
//!
//! `docs/superpowers/specs/2026-08-22-compartilhamento-de-tela-design.md` §5.1,
//! decidido em 22/08/2026: **o servidor encaminha.** A alternativa B pedia um
//! caminho cliente↔cliente que este produto nunca teve e só trocaria quem paga;
//! a C — só quem hospeda compartilha — entregava o recurso pela metade.
//!
//! A onda 1 deixou o plano de controle inteiro (quem começou, quem parou, quem
//! pediu quadro-chave) e **nada bombeando os bytes**. Isto é a bomba.
//!
//! # Fluxo, e nunca datagrama
//!
//! O §3.1 é medido e não argumentado: `send_datagram` põe voz e vídeo na mesma
//! fila FIFO do `quinn-proto`, que descarta o **mais velho** quando enche —
//! 16,1% da voz perdida e 2,16 s de atraso com o buffer padrão, e 98,1%
//! descartada ao encolher o buffer. Nada aqui toca em datagrama: o que chega
//! num fluxo unidirecional sai em fluxos unidirecionais.
//!
//! # O servidor nunca olha dentro do quadro
//!
//! É a mesma regra que `specs/04-servidor-seele.md` dá para o Opus, e pelo
//! mesmo motivo: é ela que mantém a CPU do Dogma plana e que deixa o E2EE de
//! mídia (`specs/09`) ser um acréscimo em vez de uma reescrita. O que o
//! [`Enquadramento`] lê são os cinco bytes que separam um quadro do outro —
//! tipo e tamanho — e nada além disso. Cinco bytes não são um decodificador.
//!
//! # Por que estas constantes estão repetidas
//!
//! O ADR 0002 proíbe o daemon de depender do `seele-core`, que é o *cliente*.
//! `CABECALHO_DE_QUADRO_LEN`, `MAX_QUADRO_LEN`, [`FRACAO_DO_CAMINHO`] e
//! [`PISO_DE_BANDA_BPS`] existem lá em `seele_core::tela` com estes mesmos
//! valores, pela mesma razão que `crate::frame` é gêmeo de `seele_core::frame`
//! e que o balde de bytes é gêmeo do daqui: quarenta linhas repetidas custam
//! menos que um crate de transporte que os dois dependeriam e nenhum seria
//! dono. **O que não pode divergir é o formato**, e ele está escrito nos dois
//! lados a partir do mesmo §3.

use seele_proto::ids::ScreenId;
use tokio::sync::mpsc;

/// Bytes de cabeçalho na frente de cada quadro codificado.
///
/// Um byte de tipo e quatro de tamanho, big-endian. Gêmeo de
/// `seele_core::tela::CABECALHO_DE_QUADRO_LEN` — ver o cabeçalho deste módulo.
pub const CABECALHO_DE_QUADRO_LEN: usize = 5;

/// Maior quadro codificado que este Dogma repassa, em bytes.
///
/// `specs/08-seguranca.md`: o tamanho anunciado por um par é conferido **antes**
/// de qualquer alocação. Aqui ele não aloca nada — o encaminhamento é por
/// pedaço, sem remontar quadro —, mas um tamanho absurdo é a única coisa que
/// distingue um fluxo de tela de um fluxo de lixo, e um enquadramento que
/// aceitasse 4 GiB nunca perceberia que perdeu o passo.
pub const MAX_QUADRO_LEN: usize = 512 * 1024;

/// Que fração do caminho medido o vídeo pode ocupar, em por cento.
///
/// 60, e é medida: com o vídeo pedindo 1200 kbps num caminho de 2000, a voz
/// volta para 23,1 ms de p50 e 0% de perda; solto, ela vai a 225,7 ms no mesmo
/// cano (§3.2).
pub const FRACAO_DO_CAMINHO: u32 = 60;

/// Abaixo deste teto o compartilhamento **para**, em bits por segundo.
///
/// §2 pede piso com nome: *«se o encoder não sustenta nem o piso, o
/// compartilhamento para, com motivo enumerado»*. O número é extrapolação —
/// nenhuma linha de `spikes/tela-no-codec` rodou abaixo de 1200 kbps de teto —
/// e está aqui com o mesmo valor que `seele_core::tela::PISO_DE_BANDA_BPS`.
pub const PISO_DE_BANDA_BPS: u32 = 200_000;

/// O caminho de subida que se **assume** para este Dogma, em bits por segundo.
///
/// **Hipótese, e escrita como hipótese.** O §8 pergunta 2 continua aberta —
/// ninguém mede quanto cabe num caminho que não está sendo enchido — e o
/// produto não tem resposta. Assume-se o cano sobre o qual as duas provas
/// rodaram, 2000 kbps de subida, que é a única suposição com número atrás.
///
/// É o número que o §5.1 chama de *«caminho de quem hospeda»*, e é a perna que
/// o produto até agora **não media**: o teto saía do caminho de quem
/// compartilha, e com o servidor encaminhando é a subida do Dogma que estoura
/// primeiro.
///
/// Só a admissão deste lado sai daqui. **No fio ele não vai** — ver
/// [`caminho_no_fio`], e a diferença entre os dois é o assunto inteiro destas
/// vinte linhas.
pub const CAMINHO_DO_DOGMA_BPS: u32 = 2_000_000;

/// A subida por que este Dogma divide N ao admitir uma transmissão.
///
/// O que o operador declarou, ou a hipótese de [`CAMINHO_DO_DOGMA_BPS`].
/// Zero declarado é tratado como nada declarado: um caminho de zero bit por
/// segundo pararia toda transmissão deste Dogma, e um campo de configuração em
/// branco não é um pedido para desligar o recurso.
///
/// Recusar-se a admitir sem um número seria a outra escolha, e é pior: um
/// Dogma que não sabe a própria subida ainda tem de decidir o que faz quando
/// alguém aperta o botão, e a hipótese de 2000 kbps é conservadora — ela erra
/// para o lado de encerrar cedo, que é o lado que o §3.2 manda errar.
#[must_use]
pub fn caminho_do_dogma(declarado: Option<u32>) -> u32 {
    declarado
        .filter(|bps| *bps > 0)
        .unwrap_or(CAMINHO_DO_DOGMA_BPS)
}

/// O que o Dogma diz da própria subida no `HostUplink`, em bits por segundo.
///
/// **Zero quer dizer «não medi»**, o mesmo contrato do `——` que o resto do
/// produto usa, e quem recebe trata isso como ausência — o termo do §5.1 some
/// do `min` em vez de virar um teto de zero.
///
/// A diferença para [`caminho_do_dogma`] é que a hipótese **não atravessa o
/// fio**. Dentro desta máquina ela é uma decisão de admissão, e assumir é o que
/// se faz na falta de medida; posta no fio ela vira uma promessa de banda que
/// ninguém conferiu, e o cliente a usaria para escolher resolução. Uma medida
/// inventada é pior que a ausência declarada.
///
/// **E não há medida.** O que este Dogma sabe do próprio caminho é o que o
/// `quinn` conta por conexão — RTT, perda, janela de congestionamento —, e
/// nenhum deles diz quanto **cabe** num cano que não está sendo enchido, que é
/// exatamente a pergunta 2 do §8. Somar o que já saiu daria um piso
/// demonstrado, não uma capacidade: num Dogma parado ele desabaria abaixo do
/// piso do §2 e pararia transmissões que cabiam. Então: o que o operador
/// declarou, ou nada.
#[must_use]
pub fn caminho_no_fio(declarado: Option<u32>) -> u32 {
    declarado.filter(|bps| *bps > 0).unwrap_or(0)
}

/// Quantas aberturas de transmissão esperam por espectador.
///
/// Abrir é raro — uma por transmissão —, então isto é folga e não medida. O
/// que precisa de fila é o corpo, e ele tem a sua em [`Pedaco`].
pub const ABERTURAS_DEPTH: usize = 4;

/// Quantos pedaços de tela esperam por espectador antes do corte.
///
/// O teto de memória do Dogma sai daqui: no pior caso são
/// `ABERTURAS_DEPTH + PEDACOS_DEPTH` pedaços de [`LEITURA_LEN`] por espectador,
/// meio megabyte, e um Dogma é dimensionado em 512 MB
/// (`specs/04-servidor-seele.md`). Cheia, a fila **corta aquele espectador** —
/// nunca descarta um pedaço — pelo motivo que [`Pedaco`] escreve.
pub const PEDACOS_DEPTH: usize = 64;

/// Quantos bytes do fluxo de quem compartilha se lê de uma vez.
pub const LEITURA_LEN: usize = 8 * 1024;

/// Prioridade do fluxo de tela, abaixo de tudo o mais que o Dogma escreve.
///
/// O controle é `crate::transfer::CONTROL_PRIORITY` e as transferências são
/// `TRANSFER_PRIORITY`. A tela fica abaixo das duas, e a ordem importa menos do
/// que parece: o §3.2 é explícito em que prioridade dentro do QUIC **não
/// alcança** a fila do gargalo, que é onde a voz sofre. Isto só arruma a ordem
/// de saída desta máquina.
pub const PRIORIDADE_DA_TELA: i32 = -2;

/// Código com que um fluxo de tela é cortado.
///
/// Cortar e não terminar, e a diferença é a frase que quem assiste lê: um fluxo
/// **terminado** é «a transmissão acabou», e um fluxo **cortado** é «a sua
/// cópia se perdeu». Terminar um fluxo truncado ensinaria o espectador a
/// chamar de fim o que foi uma queda.
pub const CODIGO_DE_CORTE: u32 = 1;

/// O teto que a subida do Dogma impõe a cada cópia, em bits por segundo.
///
/// É a **primeira linha** do `min` do §5.1, e é a linha que faltava:
///
/// ```text
/// teto = min(
///     caminho de quem HOSPEDA × 60% ÷ N espectadores,   ← esta
///     caminho de quem COMPARTILHA × 60%,
///     o que a pessoa escolheu (§5),
/// )
/// ```
///
/// `None` quando nem [`PISO_DE_BANDA_BPS`] cabe: aí não há teto baixo, há a
/// resposta de que esta subida não carrega esta sala. Zero espectador conta
/// como um, porque uma transmissão que ninguém assiste ainda é uma transmissão
/// que a primeira pessoa a entrar vai assistir.
#[must_use]
pub fn teto_do_hospedeiro(caminho_bps: u32, espectadores: usize) -> Option<u32> {
    // Em `u64` pelo motivo que `seele_core::tela` já dá: `caminho × 60` estoura
    // `u32` a partir de uns 71 Mbit/s, que é uma fibra doméstica comum, e um
    // teto que dá a volta vira um teto minúsculo — o defeito apareceria só na
    // casa boa.
    let cabe = (u64::from(caminho_bps) * u64::from(FRACAO_DO_CAMINHO)) / 100;
    let n = espectadores.max(1) as u64;
    let por_espectador = u32::try_from(cabe / n).unwrap_or(u32::MAX);
    (por_espectador >= PISO_DE_BANDA_BPS).then_some(por_espectador)
}

/// Um pedaço de uma transmissão a caminho de um espectador.
///
/// **Não há variante de descarte, e é de propósito.** Um fluxo QUIC é uma
/// sequência ordenada de bytes: descartar um pedaço no meio não atrasa um
/// espectador, desloca o enquadramento dele para sempre — o quadro seguinte
/// leria o meio do anterior como cabeçalho. Onde o áudio descarta
/// (`Cage::forward`, «old audio helps nobody»), a tela **corta**: quem não
/// acompanha perde a transmissão inteira e sabe disso, o que é uma frase
/// verdadeira, em vez de receber lixo indistinguível de um encoder quebrado.
#[derive(Debug)]
pub enum Pedaco {
    /// Bytes crus do fluxo de quem compartilha, como chegaram.
    Bytes(Vec<u8>),
    /// A transmissão acabou por vontade de quem a mandava.
    Fim,
}

/// O convite a assistir uma transmissão, entregue à sessão de um espectador.
///
/// Um cano novo por transmissão, e não um cano por sessão: é o fechamento dele
/// que diz a [`bombear`] se o fluxo termina ou é cortado, sem uma segunda
/// bandeira que pudesse discordar do canal.
#[derive(Debug)]
pub struct AberturaDeTela {
    /// Qual transmissão.
    pub screen: ScreenId,
    /// O cabeçalho de abertura, **byte por byte como quem compartilha o
    /// escreveu**. O Dogma não o reescreve: ele já foi conferido, e o
    /// `ScreenId` dentro dele é o que o próprio Dogma atribuiu.
    pub abertura: Vec<u8>,
    /// Por onde o corpo chega.
    pub pedacos: mpsc::Receiver<Pedaco>,
}

/// Por que o Dogma encerrou uma transmissão que ninguém mandou parar.
///
/// Enumerado, como `specs/02-protocolo.md` manda em toda razão: quem recebe
/// isto tem de escrever uma frase, e uma string de erro não deixa.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FimDaTela {
    /// A sala cresceu além do que a subida deste Dogma carrega.
    ///
    /// §5.1: a subida do hospedeiro é `N × teto`, e com N grande o suficiente
    /// nem o piso do §2 cabe. Parar é a escalada que o §3.2 escreve — *«quando
    /// o sinal cai de faixa, quem baixa é o vídeo; se continuar caindo, quem
    /// para é o vídeo»* —, e a alternativa seria a sala inteira picotando por
    /// causa da tela, que é o produto quebrado.
    AlemDoQueOHospedeiroCarrega,
    /// O fluxo deixou de ser um fluxo de tela.
    ///
    /// Um tamanho de quadro impossível ou um byte de tipo que não é 0 nem 1.
    /// Encaminhar depois disso seria encaminhar lixo para N pessoas.
    FluxoMalformado,
}

/// Onde o enquadramento de um fluxo de tela está, quadro a quadro.
///
/// Existe por causa de uma pergunta só, e ela é o §5.1 em movimento: **gente
/// entra na sala no meio da transmissão.** Um espectador ligado num byte
/// qualquer leria o meio de um quadro como cabeçalho e nunca mais acertaria o
/// passo. Ligado num começo de **quadro-chave** ele acerta o passo e ainda
/// consegue decodificar, que são as duas coisas de que precisa.
///
/// O que ele **não** faz é remontar quadro. Remontar o quadro-chave para
/// reenviá-lo inteiro desfaria o §3.3, que é a medida mais barata de toda a
/// spec: espalhar o mesmo quadro-chave em quatro tiques leva o p95 da voz de
/// 78,9 para 35,8 ms **com o mesmo bitrate entregue**. O Dogma repassa o pedaço
/// que chegou, quando chegou, e só conta os bytes para saber onde está.
#[derive(Debug, Default)]
pub struct Enquadramento {
    /// Quantos bytes faltam do quadro que está passando.
    restam: usize,
    /// O cabeçalho do próximo quadro, enquanto ele chega partido em dois
    /// pedaços.
    cabecalho: Vec<u8>,
}

impl Enquadramento {
    /// Um enquadramento no começo de um fluxo, esperando o primeiro cabeçalho.
    #[must_use]
    pub fn novo() -> Self {
        Self::default()
    }

    /// Passa um pedaço pelo enquadramento e diz onde alguém pode entrar.
    ///
    /// Devolve o deslocamento, dentro deste pedaço, do primeiro cabeçalho de
    /// **quadro-chave que começa e termina neste mesmo pedaço**. Um cabeçalho
    /// partido entre dois pedaços não vira porta de entrada: os bytes da
    /// primeira metade já saíram, e quem entrasse agora receberia a segunda
    /// metade de um cabeçalho como se fosse a primeira. Perder essa
    /// oportunidade custa um quadro-chave de espera, e quem entra pede um
    /// (`ClientMessage::RequestKeyFrame`, que a onda 1 já atende).
    ///
    /// # Errors
    ///
    /// [`FimDaTela::FluxoMalformado`] para um tamanho de quadro fora de
    /// [`MAX_QUADRO_LEN`], um quadro vazio, ou um byte de tipo que não é 0 nem
    /// 1.
    pub fn entrada(&mut self, bytes: &[u8]) -> Result<Option<usize>, FimDaTela> {
        let mut entrada = None;
        let mut i = 0;
        while i < bytes.len() {
            if self.restam > 0 {
                let anda = self.restam.min(bytes.len().saturating_sub(i));
                self.restam -= anda;
                i += anda;
                continue;
            }
            let comeca_aqui = self.cabecalho.is_empty();
            let inicio = i;
            let falta = CABECALHO_DE_QUADRO_LEN.saturating_sub(self.cabecalho.len());
            let anda = falta.min(bytes.len().saturating_sub(i));
            self.cabecalho
                .extend_from_slice(bytes.get(i..i.saturating_add(anda)).unwrap_or_default());
            i += anda;
            if self.cabecalho.len() < CABECALHO_DE_QUADRO_LEN {
                break;
            }
            let tipo = self.cabecalho.first().copied().unwrap_or(u8::MAX);
            let tamanho = self
                .cabecalho
                .get(1..CABECALHO_DE_QUADRO_LEN)
                .and_then(|quatro| <[u8; 4]>::try_from(quatro).ok())
                .map_or(0, u32::from_be_bytes) as usize;
            self.cabecalho.clear();
            // O byte de tipo é `u8::from(chave)` do outro lado, então 0 ou 1 e
            // nada mais. Aceitar 2 seria aceitar que este fluxo já não é o que
            // se pensa que é, e o resto da leitura seria adivinhação.
            if tipo > 1 || tamanho == 0 || tamanho > MAX_QUADRO_LEN {
                return Err(FimDaTela::FluxoMalformado);
            }
            self.restam = tamanho;
            if tipo == 1 && comeca_aqui && entrada.is_none() {
                entrada = Some(inicio);
            }
        }
        Ok(entrada)
    }
}

/// Escreve as transmissões de uma sala no fluxo de um espectador.
///
/// Uma tarefa por sessão, dona da conexão de saída daquele espectador. O laço é
/// deliberadamente burro: abre um fluxo por transmissão, escreve o que vem, e
/// **termina ou corta** conforme o cano tenha dito [`Pedaco::Fim`] ou tenha
/// simplesmente fechado. Essas são as duas maneiras de uma transmissão acabar
/// para uma pessoa, e elas são frases diferentes na tela dela.
///
/// O `write_all` é onde a contrapressão mora: um espectador lento faz esta
/// tarefa parar, a fila dele encher, e o [`crate::cage::Cage`] cortá-lo. Nunca
/// faz o Dogma esperar e nunca faz os outros esperarem.
pub async fn bombear(conexao: quinn::Connection, mut aberturas: mpsc::Receiver<AberturaDeTela>) {
    while let Some(mut convite) = aberturas.recv().await {
        let Ok(mut fluxo) = conexao.open_uni().await else {
            return;
        };
        let _ = fluxo.set_priority(PRIORIDADE_DA_TELA);
        // O byte de tipo antes do cabeçalho de abertura, e a regra vale nos
        // dois sentidos: quem recebe tem um `accept_uni` só e mais de um uso
        // para ele. Separar por aritmética sobre o conteúdo é o que o §5.2
        // chama de dívida, e o pior erro de protocolo que existe é um fluxo
        // lido como o tipo errado.
        let marca = [seele_proto::stream::StreamType::Screen.byte()];
        if fluxo.write_all(&marca).await.is_err() {
            continue;
        }
        if fluxo.write_all(&convite.abertura).await.is_err() {
            continue;
        }
        let mut limpo = false;
        while let Some(pedaco) = convite.pedacos.recv().await {
            match pedaco {
                Pedaco::Bytes(bytes) => {
                    if fluxo.write_all(&bytes).await.is_err() {
                        break;
                    }
                }
                Pedaco::Fim => {
                    limpo = true;
                    break;
                }
            }
        }
        if limpo {
            let _ = fluxo.finish();
        } else {
            // `reset` e não `finish`: ver [`CODIGO_DE_CORTE`].
            let _ = fluxo.reset(quinn::VarInt::from_u32(CODIGO_DE_CORTE));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Um quadro como `seele_core::tela` o escreve: tipo, tamanho, corpo.
    fn quadro(chave: bool, tamanho: usize) -> Vec<u8> {
        let mut bytes = vec![u8::from(chave)];
        bytes.extend_from_slice(&(tamanho as u32).to_be_bytes());
        bytes.extend(std::iter::repeat_n(0xAB, tamanho));
        bytes
    }

    #[test]
    fn o_teto_do_hospedeiro_e_dividido_pelos_espectadores() {
        // A primeira linha do min do §5.1, e a razão inteira desta onda: com o
        // servidor encaminhando, a subida que estoura é a dele.
        assert_eq!(teto_do_hospedeiro(2_000_000, 1), Some(1_200_000));
        assert_eq!(teto_do_hospedeiro(2_000_000, 4), Some(300_000));
        // Zero espectador conta como um: quem entrar daqui a um segundo assiste
        // à mesma transmissão.
        assert_eq!(
            teto_do_hospedeiro(2_000_000, 0),
            teto_do_hospedeiro(2_000_000, 1)
        );
    }

    #[test]
    fn a_hipotese_admite_aqui_dentro_e_nao_atravessa_o_fio() {
        // As duas respostas à mesma pergunta, e elas **têm** de discordar. Sem
        // número declarado, a admissão daqui cai na hipótese das provas — que
        // erra para o lado de encerrar cedo — e o fio leva zero, que pelo §5.1
        // quer dizer «não medi» e faz o termo sumir do `min` do outro lado.
        // Mandar a hipótese seria prometer 2000 kbps que ninguém conferiu.
        assert_eq!(caminho_do_dogma(None), CAMINHO_DO_DOGMA_BPS);
        assert_eq!(caminho_no_fio(None), 0);
    }

    #[test]
    fn o_que_o_operador_declara_vale_nos_dois_lugares() {
        // Declarado, as duas contas partem do mesmo número — que é a regra 2 do
        // §3.2: nada de um segundo medidor discordando do primeiro.
        assert_eq!(caminho_do_dogma(Some(50_000_000)), 50_000_000);
        assert_eq!(caminho_no_fio(Some(50_000_000)), 50_000_000);
    }

    #[test]
    fn zero_declarado_e_um_campo_em_branco_e_nao_um_pedido_para_desligar() {
        // Um caminho de zero bit por segundo pararia toda transmissão deste
        // Dogma, e no fio ele já quer dizer «não medi». Ler os dois como
        // ausência é o que impede uma configuração meio preenchida de desligar
        // o recurso em silêncio.
        assert_eq!(caminho_do_dogma(Some(0)), CAMINHO_DO_DOGMA_BPS);
        assert_eq!(caminho_no_fio(Some(0)), 0);
    }

    #[test]
    fn abaixo_do_piso_nao_ha_teto_baixo_ha_parada() {
        // §2: «se o encoder não sustenta nem o piso, o compartilhamento para,
        // com motivo enumerado». Um teto de 170 kbps devolvido como número
        // faria o produto prometer uma imagem que não existe.
        assert_eq!(teto_do_hospedeiro(2_000_000, 6), Some(PISO_DE_BANDA_BPS));
        assert_eq!(teto_do_hospedeiro(2_000_000, 7), None);
    }

    #[test]
    fn o_teto_nao_da_a_volta_numa_casa_de_fibra() {
        // `caminho × 60` estoura `u32` a partir de uns 71 Mbit/s. Feito em
        // `u32` isto devolveria um teto minúsculo, e só na casa boa.
        assert_eq!(teto_do_hospedeiro(900_000_000, 1), Some(540_000_000));
    }

    #[test]
    fn a_porta_de_entrada_e_o_comeco_de_um_quadro_chave() {
        let mut enq = Enquadramento::novo();
        let mut fluxo = quadro(false, 10);
        let onde = fluxo.len();
        fluxo.extend(quadro(true, 20));
        fluxo.extend(quadro(false, 5));
        assert_eq!(enq.entrada(&fluxo), Ok(Some(onde)));
    }

    #[test]
    fn sem_quadro_chave_nao_ha_porta() {
        let mut enq = Enquadramento::novo();
        let mut fluxo = quadro(false, 10);
        fluxo.extend(quadro(false, 20));
        assert_eq!(enq.entrada(&fluxo), Ok(None));
    }

    #[test]
    fn um_cabecalho_partido_ao_meio_nao_vira_porta() {
        // O caso que uma divisão exata esconderia: os bytes da primeira metade
        // do cabeçalho já saíram para quem já assistia, e ligar alguém agora o
        // faria ler a segunda metade como se fosse a primeira. A porta é
        // pulada; quem entrou pede um quadro-chave e espera o próximo.
        let mut enq = Enquadramento::novo();
        let chave = quadro(true, 20);
        assert_eq!(enq.entrada(chave.get(..3).unwrap_or_default()), Ok(None));
        assert_eq!(enq.entrada(chave.get(3..).unwrap_or_default()), Ok(None));
        // E o passo continua certo: o quadro seguinte é reconhecido.
        assert_eq!(enq.entrada(&quadro(true, 8)), Ok(Some(0)));
    }

    #[test]
    fn o_enquadramento_atravessa_pedacos_de_qualquer_tamanho() {
        // O tamanho do pedaço é do QUIC, não nosso: o mesmo fluxo tem de dar a
        // mesma resposta byte a byte e de uma vez só.
        let mut fluxo = quadro(false, 300);
        let onde = fluxo.len();
        fluxo.extend(quadro(true, 100));
        let mut inteiro = Enquadramento::novo();
        assert_eq!(inteiro.entrada(&fluxo), Ok(Some(onde)));

        let mut picado = Enquadramento::novo();
        let mut achado = None;
        for (i, byte) in fluxo.iter().enumerate() {
            if let Ok(Some(_)) = picado.entrada(&[*byte]) {
                achado = Some(i);
            }
        }
        // Um cabeçalho que chega byte a byte nunca começa e termina no mesmo
        // pedaço, então não há porta — e é exatamente o que a regra diz.
        assert_eq!(achado, None);
    }

    #[test]
    fn um_tamanho_impossivel_encerra_o_fluxo() {
        let mut enq = Enquadramento::novo();
        let mut cabecalho = vec![0_u8];
        cabecalho.extend_from_slice(&(MAX_QUADRO_LEN as u32 + 1).to_be_bytes());
        assert_eq!(enq.entrada(&cabecalho), Err(FimDaTela::FluxoMalformado));
    }

    #[test]
    fn um_quadro_vazio_encerra_o_fluxo() {
        let mut enq = Enquadramento::novo();
        assert_eq!(
            enq.entrada(&[0, 0, 0, 0, 0]),
            Err(FimDaTela::FluxoMalformado)
        );
    }

    #[test]
    fn um_byte_de_tipo_que_nao_e_zero_nem_um_encerra_o_fluxo() {
        let mut enq = Enquadramento::novo();
        assert_eq!(
            enq.entrada(&[2, 0, 0, 0, 8]),
            Err(FimDaTela::FluxoMalformado)
        );
    }
}
