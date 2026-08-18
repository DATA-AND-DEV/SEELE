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

use std::io;
use std::net::{SocketAddr, UdpSocket};

use seele_proto::encontro::{responder_em, Vizinhanca, TAMANHO};

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

/// Um ponto de encontro escutando numa porta.
#[derive(Debug)]
pub struct Ponto {
    socket: UdpSocket,
    vizinhanca: Vizinhanca,
    barulhento: bool,
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
        Ok(Self {
            socket: ligar(escuta)?,
            vizinhanca,
            barulhento: false,
        })
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

        let Some(resposta) = balde
            .get(..lidos)
            .and_then(|recebido| responder_em(recebido, de, self.vizinhanca))
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
