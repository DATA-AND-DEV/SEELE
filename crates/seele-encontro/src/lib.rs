//! O ponto de encontro do degrau 4 do ADR 0022.
//!
//! Ele apresenta duas máquinas atrás de NAT uma à outra e sai do caminho. O
//! áudio e o texto **nunca** passam por aqui: quando este processo já não
//! importa mais é que a conversa começa.
//!
//! # O que ele é
//!
//! Um laço que recebe um datagrama, chama [`seele_proto::encontro::responder`],
//! manda o que ela disser e esquece tudo. Não tem banco, não tem tabela, não
//! tem arquivo, não tem sessão; reiniciá-lo no meio de uma apresentação custa
//! uma repetição de 96 bytes.
//!
//! É de propósito que ele seja assim tão pequeno. O ADR 0022 aceita este degrau
//! com a condição de o ponto de encontro ser **trocável** — quem hospeda aponta
//! para o seu, e o endereço vai no `seele://`. Um serviço difícil de subir
//! transformaria essa condição numa frase educada que ninguém exerce.
//!
//! # O que ele aprende, e o que ele guarda
//!
//! Aprende **metadado**: que endereço falou com que endereço, e quando. É o
//! custo que o ADR 0022 nomeia em voz alta, e é real.
//!
//! Guarda **nada**. Por padrão nem imprime: o registro de quem falou com quem é
//! justamente a coisa que este projeto não quer que exista, e um log ligado por
//! padrão seria criá-la por conveniência de quem depura. `--barulhento` existe
//! para quem estiver investigando um problema, e diz na cara o que passa a
//! escrever.
//!
//! # Duas famílias, dois sockets
//!
//! Sem `socket2` e sem confiar no `IPV6_V6ONLY`, cujo padrão muda de sistema
//! para sistema — o degrau 2 deste mesmo ADR mediu isso e apanhou. Um socket
//! IPv4 e um IPv6, cada um numa linha de execução, é a versão que funciona
//! igual no Linux, no BSD e no Windows sem dependência nenhuma.

use std::collections::HashMap;
use std::io;
use std::net::{SocketAddr, UdpSocket};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use seele_proto::encontro::{analisar, aqui, responder_em, Pedido, Vizinhanca, TAMANHO};

/// O que aconteceu com um datagrama.
///
/// Existe para o teste poder afirmar as duas coisas que importam: que um pedido
/// legítimo foi atendido, e que o resto foi **calado** — nunca respondido "não".
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Movimento {
    /// Alguém foi apresentado a alguém.
    Levado {
        /// De onde veio o pedido.
        de: SocketAddr,
        /// Para onde o aviso foi.
        para: SocketAddr,
    },
    /// O datagrama não era deste protocolo, ou pedia o que não se faz.
    ///
    /// A resposta é o silêncio, e não uma recusa: um refletor que responde a
    /// tudo é útil para quem estiver medindo o que ele responde.
    Calado {
        /// De onde veio.
        de: SocketAddr,
    },
}

/// Por quanto tempo um endereço registrado com `MORO` continua valendo.
///
/// Quatro vezes o intervalo de reavivamento do anfitrião, que é de quinze
/// segundos. Três pacotes perdidos seguidos não podem apagar quem está no ar; um
/// servidor que fechou não pode continuar sendo apontado por minutos.
const PRAZO_DO_QUARTO: Duration = Duration::from_secs(60);

/// Quantas marcas cabem no quarto ao mesmo tempo.
///
/// **É um teto de memória, e ele existe porque escrever aqui é de graça.**
/// Qualquer um manda `MORO` com a marca que inventar, e sem teto isso é um jeito
/// de encher a RAM de quem opera o serviço com 96 bytes por vez. Cheio, o quarto
/// primeiro varre o que venceu; se ainda estiver cheio, recusa a marca nova em
/// silêncio — quem está dentro já está no ar, e derrubar quem funciona para
/// caber quem acabou de chegar é a troca errada.
const MARCAS_NO_QUARTO: usize = 4096;

/// Onde cada marca disse que mora, e desde quando.
///
/// **A única memória deste processo, e o ADR 0022 a recusava.** A revisão de
/// 03/09/2026 está escrita no cabeçalho de `seele_proto::encontro`, com o que
/// ela custa. O resumo do que vive aqui: um mapa de meia impressão digital para
/// um endereço, em RAM, com prazo. Nunca em disco. Nunca no `--barulhento`.
///
/// `Mutex` e não `RwLock`: as duas famílias são duas linhas de execução e cada
/// datagrama toca o mapa uma vez, então não há leitura concorrente longa a
/// proteger. Um `RwLock` aqui seria complexidade paga por nada.
#[derive(Debug, Default)]
pub struct Quarto {
    moradores: Mutex<HashMap<String, (SocketAddr, Instant)>>,
}

impl Quarto {
    /// Um quarto vazio.
    #[must_use]
    pub fn novo() -> Self {
        Self::default()
    }

    /// Registra que esta marca mora neste endereço, ou recusa em silêncio.
    ///
    /// **Quem escreveu primeiro fica, enquanto o prazo não vencer.** É a defesa
    /// contra tomar o lugar de alguém, e ela não é autenticação — este serviço
    /// não tem chave nenhuma para conferir. Ver o cabeçalho de
    /// `seele_proto::encontro`: o anfitrião reavive o dele a cada quinze
    /// segundos, então o lugar só está livre quando ele está fora do ar.
    ///
    /// O próprio morador **pode** se mudar: é do mesmo endereço que ele reavive,
    /// e é para o endereço novo que ele escreve quando o NAT lhe deu outro.
    fn morar(&self, marca: &str, onde: SocketAddr) {
        let Ok(mut moradores) = self.moradores.lock() else {
            return;
        };
        let agora = Instant::now();

        match moradores.get(marca) {
            // Quem já mora aqui reavive, e pode ter mudado de endereço.
            Some((antigo, _)) if antigo.ip() == onde.ip() => {}
            // Outro endereço, e o prazo do morador ainda não venceu: fica quem
            // estava. O IP e não o socket inteiro, porque a porta é justamente
            // o que muda — comparar o socket faria todo remapeamento de NAT
            // parecer um impostor.
            Some((_, desde)) if agora.duration_since(*desde) < PRAZO_DO_QUARTO => return,
            Some(_) | None => {}
        }

        if moradores.len() >= MARCAS_NO_QUARTO && !moradores.contains_key(marca) {
            moradores.retain(|_, (_, desde)| agora.duration_since(*desde) < PRAZO_DO_QUARTO);
            if moradores.len() >= MARCAS_NO_QUARTO {
                return;
            }
        }
        moradores.insert(marca.to_owned(), (onde, agora));
    }

    /// Onde esta marca mora, se ela morar em algum lugar e o prazo não venceu.
    fn onde_mora(&self, marca: &str) -> Option<SocketAddr> {
        let moradores = self.moradores.lock().ok()?;
        let (onde, desde) = moradores.get(marca)?;
        (Instant::now().duration_since(*desde) < PRAZO_DO_QUARTO).then_some(*onde)
    }

    /// Quantas marcas moram aqui agora. Para o teste, e para nada mais.
    #[must_use]
    pub fn quantos(&self) -> usize {
        self.moradores.lock().map_or(0, |m| m.len())
    }
}

/// Um ponto de encontro escutando numa porta.
#[derive(Debug)]
pub struct Ponto {
    socket: UdpSocket,
    vizinhanca: Vizinhanca,
    barulhento: bool,
    /// Compartilhado entre as duas famílias: um anfitrião registra por uma e
    /// quem procura pode perguntar pela outra.
    quarto: Arc<Quarto>,
}

/// Liga o socket, dizendo ao sistema qual família ele atende.
///
/// # Por que não é `UdpSocket::bind`
///
/// Este processo abre **uma escuta por família**, e para isso o socket IPv6
/// precisa ser exclusivamente IPv6. Não dizer isso não deixa a decisão em
/// aberto: o Linux resolve por conta própria e faz o socket de pilha dupla, que
/// tenta cobrir IPv4 também, colide com o IPv4 que já subiu, e falha com
/// `Address already in use`.
///
/// O efeito é pior que uma falha barulhenta, porque o serviço **continua de
/// pé**: sobra só IPv4, o `systemctl status` diz `active (running)`, e quem
/// hospeda por IPv6 nunca é apresentado a ninguém. Foi assim que apareceu, numa
/// VPS de verdade, com o log dizendo exatamente isso e ninguém lendo o log.
///
/// A `std` não tem como marcar `IPV6_V6ONLY`, e é só para isso que o `socket2`
/// entra aqui.
fn ligar(escuta: SocketAddr) -> io::Result<UdpSocket> {
    let dominio = if escuta.is_ipv6() {
        socket2::Domain::IPV6
    } else {
        socket2::Domain::IPV4
    };
    let socket = socket2::Socket::new(dominio, socket2::Type::DGRAM, Some(socket2::Protocol::UDP))?;
    if escuta.is_ipv6() {
        socket.set_only_v6(true)?;
    }
    socket.bind(&escuta.into())?;
    Ok(socket.into())
}

impl Ponto {
    /// Abre a escuta.
    ///
    /// # Errors
    ///
    /// Falha se a porta já estiver em uso ou a família não existir nesta
    /// máquina — o caso comum de uma VPS sem IPv6.
    pub fn abrir(escuta: SocketAddr, vizinhanca: Vizinhanca) -> io::Result<Self> {
        Self::abrir_com_quarto(escuta, vizinhanca, Arc::new(Quarto::novo()))
    }

    /// O mesmo, dividindo o quarto com outra escuta.
    ///
    /// **As duas famílias têm de dividir**, ou um anfitrião que registrou por
    /// IPv4 fica invisível para quem pergunta por IPv6 — e a resposta seria o
    /// silêncio, que é indistinguível de «esse servidor não existe».
    ///
    /// # Errors
    ///
    /// O mesmo de [`Self::abrir`].
    pub fn abrir_com_quarto(
        escuta: SocketAddr,
        vizinhanca: Vizinhanca,
        quarto: Arc<Quarto>,
    ) -> io::Result<Self> {
        Ok(Self {
            socket: ligar(escuta)?,
            vizinhanca,
            barulhento: false,
            quarto,
        })
    }

    /// O quarto deste ponto, para quem quiser dividi-lo com outra escuta.
    #[must_use]
    pub fn quarto(&self) -> Arc<Quarto> {
        Arc::clone(&self.quarto)
    }

    /// Liga o registro de quem falou com quem.
    ///
    /// Desligado por padrão, e a assimetria é a decisão: este processo aprende
    /// metadado de qualquer jeito — está no caminho —, mas **escrevê-lo** é uma
    /// escolha, e a escolha padrão é não.
    #[must_use]
    pub fn barulhento(mut self, ligado: bool) -> Self {
        self.barulhento = ligado;
        self
    }

    /// Onde ele acabou escutando. Útil quando a porta pedida foi zero.
    ///
    /// # Errors
    ///
    /// Falha se o sistema não disser onde o socket ligou.
    pub fn endereco(&self) -> io::Result<SocketAddr> {
        self.socket.local_addr()
    }

    /// Recebe um datagrama, responde se for o caso, e esquece.
    ///
    /// # Errors
    ///
    /// Falha se a leitura do socket falhar. Um envio que falha **não** é erro
    /// daqui: o destino de um `LEVE` é escolhido por quem pediu, e uma rota que
    /// não existe é problema dele, não motivo para derrubar o serviço.
    pub fn atender(&self) -> io::Result<Movimento> {
        // Exatamente o tamanho do protocolo. Um datagrama maior chega truncado
        // e é recusado pelo tamanho, que é o que se quer.
        let mut balde = [0_u8; TAMANHO];
        let (lidos, de) = self.socket.recv_from(&mut balde)?;

        let recebido = balde.get(..lidos);

        // O quarto, antes da decisão sem estado. `MORO` registra e depois é
        // respondido como um `ONDE` qualquer, lá dentro; `QUEM` é a única linha
        // deste protocolo que a função livre não sabe responder, porque a
        // resposta dela não está no pedido — está aqui.
        //
        // **Nada disto é impresso, nem com `--barulhento`.** Aquela bandeira
        // liga o registro de quem falou com quem, que é metadado de passagem;
        // o quarto é metadado guardado, e escrevê-lo em disco seria fazer
        // justamente a coisa que o prazo existe para evitar.
        let do_quarto =
            recebido.and_then(analisar).and_then(|pedido| match pedido {
                Pedido::Moro { marca } => {
                    self.quarto.morar(marca.texto(), de);
                    None
                }
                Pedido::Quem { marca } => self.quarto.onde_mora(marca.texto()).map(|onde| {
                    seele_proto::encontro::Resposta {
                        destino: de,
                        datagrama: aqui(&marca, onde),
                    }
                }),
                Pedido::Onde { .. } | Pedido::Leve { .. } => None,
            });

        let Some(resposta) = do_quarto
            .or_else(|| recebido.and_then(|recebido| responder_em(recebido, de, self.vizinhanca)))
        else {
            if self.barulhento {
                eprintln!("calado para {de} ({lidos} bytes)");
            }
            return Ok(Movimento::Calado { de });
        };

        if let Err(erro) = self.socket.send_to(&resposta.datagrama, resposta.destino) {
            // Não é fatal e não vira erro: quem pediu escolheu o destino.
            if self.barulhento {
                eprintln!("não deu para avisar {}: {erro}", resposta.destino);
            }
        } else if self.barulhento {
            eprintln!("apresentei {de} a {}", resposta.destino);
        }
        Ok(Movimento::Levado {
            de,
            para: resposta.destino,
        })
    }

    /// Atende para sempre.
    ///
    /// Só volta se a **leitura** falhar, que é a única falha que não é de quem
    /// mandou o datagrama.
    ///
    /// # Errors
    ///
    /// O erro de leitura que o fez parar.
    pub fn servir(&self) -> io::Result<std::convert::Infallible> {
        loop {
            self.atender()?;
        }
    }
}

#[cfg(test)]
mod tests {
    #![allow(
        clippy::expect_used,
        clippy::unwrap_used,
        reason = "num teste, o pânico é o relatório"
    )]

    use super::*;

    #[test]
    fn as_duas_familias_cabem_na_mesma_porta() {
        // O defeito que deixou uma VPS de verdade servindo só IPv4.
        //
        // Este processo abre uma escuta por família. Sem marcar o socket IPv6
        // como exclusivo, o Linux decide por conta própria e o faz de pilha
        // dupla — ele tenta cobrir IPv4 também, colide com o IPv4 que já subiu,
        // e falha com `Address already in use`.
        //
        // O que torna isso caro é que o serviço **continua de pé**: sobra só
        // IPv4, `systemctl status` diz `active (running)`, e quem hospeda por
        // IPv6 nunca é apresentado a ninguém. O log dizia exatamente o que
        // aconteceu, e ninguém lê log de serviço que está «rodando».
        //
        // A porta é sorteada pelo sistema para o teste não brigar com o que
        // estiver na máquina — e é por isso que o IPv4 sobe primeiro e o IPv6
        // reusa a porta dele: essa é a ordem em que o defeito acontece.
        let quatro = ligar("0.0.0.0:0".parse().unwrap()).expect("IPv4 tem de subir");
        let porta = quatro.local_addr().unwrap().port();

        let seis = ligar(format!("[::]:{porta}").parse().unwrap());

        assert!(
            seis.is_ok(),
            "o IPv6 não subiu na mesma porta do IPv4: {:?}\n\
             É o socket IPv6 virando pilha dupla por conta do sistema, que é \
             exatamente o que `set_only_v6(true)` existe para impedir",
            seis.err()
        );

        // E o inverso, porque a ordem não pode ser o que segura a propriedade:
        // um dia alguém troca a ordem do laço, e o teste tem de continuar sendo
        // sobre a exclusividade e não sobre quem chegou antes.
        drop(quatro);
        drop(seis);

        let seis = ligar("[::]:0".parse().unwrap()).expect("IPv6 tem de subir");
        let porta = seis.local_addr().unwrap().port();
        assert!(
            ligar(format!("0.0.0.0:{porta}").parse().unwrap()).is_ok(),
            "o IPv4 não subiu na mesma porta do IPv6, então o IPv6 tomou as duas"
        );
    }
}

#[cfg(test)]
mod o_quarto {
    #![allow(
        clippy::expect_used,
        clippy::unwrap_used,
        reason = "num teste, o pânico é o relatório"
    )]

    use super::*;

    fn em(porta: u16) -> SocketAddr {
        SocketAddr::from(([203, 0, 113, 7], porta))
    }

    #[test]
    fn quem_mora_muda_de_porta_e_continua_sendo_encontrado() {
        // **É a razão inteira de o quarto existir.** O mapeamento de NAT do
        // anfitrião nasce quando um pacote sai, e o roteador dá outro na
        // abertura seguinte. Sem isto o endereço guardado na lista de servidores
        // conhecidos morre no fechar, e a lista fica inútil para quem está atrás
        // de NAT — que foi exatamente o relato.
        let quarto = Quarto::novo();
        quarto.morar("abc123", em(40000));
        assert_eq!(quarto.onde_mora("abc123"), Some(em(40000)));

        // O mesmo anfitrião, outra porta. Ele é reconhecido pelo IP, e não pelo
        // socket inteiro: comparar a porta faria todo remapeamento parecer um
        // impostor, que é o oposto do que se quer.
        quarto.morar("abc123", em(51515));
        assert_eq!(
            quarto.onde_mora("abc123"),
            Some(em(51515)),
            "o anfitrião mudou de porta e o quarto ficou com a antiga — é o \
             defeito que ele existe para não ter"
        );
    }

    #[test]
    fn quem_escreveu_primeiro_fica_enquanto_estiver_no_ar() {
        // Qualquer um manda `MORO` com a marca de outro. Isto não é
        // autenticação — este serviço não tem chave nenhuma para conferir — e
        // não precisa ser: quem chega confere a impressão digital de qualquer
        // jeito (ADR 0003), então um endereço errado falha no aperto de mão.
        //
        // O que esta regra compra é que o impostor não consiga sequer isso
        // enquanto o dono estiver no ar.
        let quarto = Quarto::novo();
        quarto.morar("abc123", em(40000));

        let impostor = SocketAddr::from(([198, 51, 100, 66], 40000));
        quarto.morar("abc123", impostor);

        assert_eq!(
            quarto.onde_mora("abc123"),
            Some(em(40000)),
            "outro endereço tomou o lugar de quem estava no ar"
        );
    }

    #[test]
    fn uma_marca_que_ninguem_registrou_nao_tem_endereco() {
        // Calar, e nunca responder «não»: é a regra deste módulo inteiro, e
        // aqui ela também é correção — um endereço inventado mandaria quem
        // perguntou para o lugar errado.
        let quarto = Quarto::novo();
        assert_eq!(quarto.onde_mora("naovive"), None);
    }

    #[test]
    fn o_quarto_tem_teto_porque_escrever_nele_e_de_graca() {
        // Qualquer um manda `MORO` com a marca que inventar, e sem teto isso é
        // um jeito de encher a RAM de quem opera o serviço, 96 bytes por vez.
        let quarto = Quarto::novo();
        for i in 0..(MARCAS_NO_QUARTO + 500) {
            quarto.morar(&format!("m{i}"), em(40000));
        }
        assert!(
            quarto.quantos() <= MARCAS_NO_QUARTO,
            "o quarto passou do teto: {} marcas",
            quarto.quantos()
        );

        // E quem entrou primeiro continua lá: derrubar quem está no ar para
        // caber quem acabou de chegar é a troca errada.
        assert_eq!(
            quarto.onde_mora("m0"),
            Some(em(40000)),
            "encher o quarto expulsou quem já estava dentro"
        );
    }

    #[test]
    fn o_ponto_responde_quem_com_o_endereco_que_moro_registrou() {
        // As duas pontas do protocolo, pelo socket de verdade: um `MORO` de um
        // lado e um `QUEM` do outro, com o quarto dividido entre eles — que é
        // como as duas famílias funcionam.
        let ponto = Ponto::abrir(
            SocketAddr::from(([127, 0, 0, 1], 0)),
            Vizinhanca::TambemAqui,
        )
        .expect("a escuta tem que abrir");
        let onde = ponto.endereco().expect("o socket sabe onde ligou");

        let marca = seele_proto::encontro::Marca::nova("abc123").expect("é uma marca");

        let anfitriao = UdpSocket::bind(SocketAddr::from(([127, 0, 0, 1], 0))).unwrap();
        anfitriao
            .send_to(&seele_proto::encontro::moro(&marca), onde)
            .unwrap();
        ponto.atender().expect("o MORO tem que ser atendido");
        // O `MORO` responde como um `ONDE`: quem registra também aprende onde
        // está, e é isso que faz ele caber no pacote de reavivamento.
        let mut balde = [0_u8; TAMANHO];
        anfitriao
            .set_read_timeout(Some(Duration::from_secs(2)))
            .unwrap();
        anfitriao.recv_from(&mut balde).expect("o MORO responde");

        let quem_procura = UdpSocket::bind(SocketAddr::from(([127, 0, 0, 1], 0))).unwrap();
        quem_procura
            .send_to(&seele_proto::encontro::quem(&marca), onde)
            .unwrap();
        ponto.atender().expect("o QUEM tem que ser atendido");

        let mut resposta = [0_u8; TAMANHO];
        quem_procura
            .set_read_timeout(Some(Duration::from_secs(2)))
            .unwrap();
        quem_procura
            .recv_from(&mut resposta)
            .expect("quem pergunta por uma marca que mora aqui tem que ser respondido");

        let texto = String::from_utf8_lossy(&resposta);
        let esperado = anfitriao.local_addr().unwrap().to_string();
        assert!(
            texto.contains(&esperado),
            "a resposta tinha que trazer o endereço de quem registrou: {texto}"
        );
    }
}
