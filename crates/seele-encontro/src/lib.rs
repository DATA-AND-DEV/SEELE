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

impl Ponto {
    /// Abre a escuta.
    ///
    /// # Errors
    ///
    /// Falha se a porta já estiver em uso ou a família não existir nesta
    /// máquina — o caso comum de uma VPS sem IPv6.
    pub fn abrir(escuta: SocketAddr, vizinhanca: Vizinhanca) -> io::Result<Self> {
        Ok(Self {
            socket: UdpSocket::bind(escuta)?,
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
