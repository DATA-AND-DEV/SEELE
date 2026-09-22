//! What a MOD reaches that is not the protocol: network, clock and log.
//!
//! ADR 0045, the `world` block. This is where "liberdade total" lives — a MOD
//! opens outbound connections from the machine of whoever hosts, which is what
//! the owner's tunnel MOD needs and what the acceptance screen has to say out
//! loud.
//!
//! # The ceiling that the interpreter's does not cover
//!
//! `set_interrupt_handler` fires while **JavaScript** runs. It does not fire
//! inside a native call, so a MOD sitting in `buscar()` is not interrupted by
//! the step ceiling at all — and the dispatcher is serial on purpose, so one
//! slow endpoint would stall every MOD and the whole event queue behind it.
//!
//! That is why the request carries a ceiling of its own, and why it is small.
//! It is not belt and braces: without it the step ceiling has a hole exactly
//! the size of the network.

#![cfg_attr(test, allow(clippy::unwrap_used, clippy::expect_used))]

/// How long one request may take.
///
/// Small because of what it holds up: the MOD thread runs one moment at a time,
/// so this is the worst case a single MOD can delay every other MOD **and** the
/// events queued behind them. Two seconds is already long for a room where
/// people are talking; more would be a MOD deciding how responsive the server
/// is.
pub const TETO_DA_BUSCA: std::time::Duration = std::time::Duration::from_secs(2);

/// The most one response body a MOD receives may hold, in bytes.
///
/// A MOD that asks for a gigabyte would hold the ceiling above *and* the memory
/// of the whole server, which is not the MOD's to spend.
pub const TETO_DA_RESPOSTA: usize = 4 * 1024 * 1024;

/// Fetches one URL, or nothing.
///
/// Deliberately one answer for every failure — refused scheme, DNS, timeout,
/// oversized body. A MOD that could tell them apart could map the network of
/// whoever hosts it, and it has no use for the difference.
#[must_use]
pub fn buscar(url: &str) -> Option<String> {
    // `http` e `https` e mais nada. Sem isto, `file:///…/identity.key` faria
    // pela rede exatamente o que a pasta do MOD existe para impedir no disco.
    if !url.starts_with("http://") && !url.starts_with("https://") {
        return None;
    }

    // O provedor de criptografia, pelo mesmo idioma que este crate já usa em
    // `lib.rs` e que o `seele-core` repete em quatro lugares: `install_default`
    // devolve erro se já houver um, e o `let _` é o que torna isso idempotente.
    // Sem ele, `rustls-no-provider` **entra em pânico** ao construir o cliente —
    // e um pânico aqui derrubaria a thread dos MODs inteira, que é o oposto do
    // que a falha isolada promete.
    let _ = rustls::crypto::ring::default_provider().install_default();

    let cliente = reqwest::blocking::Client::builder()
        .timeout(TETO_DA_BUSCA)
        .build()
        .ok()?;
    let resposta = cliente.get(url).send().ok()?;

    // **O teto é aplicado enquanto os bytes chegam, e não depois deles.**
    //
    // Aqui havia `resposta.text()` e uma comparação depois. R10 da revisão da
    // v15: a alocação acontecia **antes** da conferência, e nada a limitava — o
    // teto de memória do QuickJS não alcança uma alocação nativa, e o prazo de
    // dois segundos não é um limite de bytes. Uma resposta de um gigabyte era um
    // gigabyte no servidor de quem hospeda, gasto por um MOD.
    //
    // Nem o `Content-Length` decide: ele é o que o outro lado **alega**. A
    // implementação de prévia da janela já faz assim — ver `buscar_com_teto` em
    // `apps/seele-app` —, e esta é a mesma regra no outro lado do produto.
    //
    // 64 KiB por volta: grande o bastante para não dar mil voltas num megabyte, e
    // pequeno o bastante para o corte acontecer perto do teto em vez de um pedaço
    // inteiro depois dele.
    let mut bytes: Vec<u8> = Vec::new();
    let mut corpo = resposta;
    let mut pedaco = [0_u8; 64 * 1024];
    loop {
        let lidos = std::io::Read::read(&mut corpo, &mut pedaco).ok()?;
        if lidos == 0 {
            break;
        }
        if bytes.len() + lidos > TETO_DA_RESPOSTA {
            // Sem `Some`: um corpo cortado é um corpo que o MOD não pediu, e
            // entregá-lo pela metade seria pior que não entregar — ele leria JSON
            // truncado como JSON inválido e culparia a outra ponta.
            return None;
        }
        bytes.extend_from_slice(pedaco.get(..lidos)?);
    }
    // Texto e não bytes, como antes: é o que a API de MOD entrega. Um corpo que
    // não é UTF-8 lê como ausência, que é a resposta única que esta função dá
    // para toda falha.
    String::from_utf8(bytes).ok()
}

/// Seconds since the Unix epoch.
#[must_use]
pub fn agora() -> i64 {
    crate::persistence::now_seconds()
}

/// Writes one line to the host's log, from a MOD.
///
/// Always prefixed by the MOD's identifier, and never by the MOD's own choice:
/// a line whose origin a MOD could forge is a line that cannot be used to
/// decide which MOD to disable.
pub fn registrar(id: &str, linha: &str) {
    // Um teto no que um MOD escreve, para que ele não vire o log inteiro.
    let recortada: String = linha.chars().take(500).collect();
    tracing::info!(mod_id = %id, "{recortada}");
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Lê o pedido HTTP até a linha em branco, e o descarta.
    ///
    /// **Necessário, e não zelo.** Fechar um socket com dados do pedido ainda no
    /// buffer de recepção manda RST em vez de FIN em várias pilhas, e aí a
    /// resposta que acabou de ser escrita é perdida — o teste falha por causa do
    /// TCP e não do código sob teste. Foi exatamente o que aconteceu na primeira
    /// versão dos dois servidores de teste abaixo.
    fn ler_o_pedido(fluxo: &mut std::net::TcpStream) {
        use std::io::Read as _;

        let mut visto = Vec::new();
        let mut byte = [0_u8; 1];
        while visto.len() < 8 * 1024 {
            match fluxo.read(&mut byte) {
                Ok(0) | Err(_) => return,
                Ok(_) => visto.push(byte[0]),
            }
            if visto.ends_with(b"\r\n\r\n") {
                return;
            }
        }
    }

    #[test]
    fn so_http_e_https_atravessam() {
        for url in [
            "file:///etc/passwd",
            "file:///Users/alguem/.config/seele/identity.key",
            "ftp://exemplo.invalido/x",
            "",
            "javascript:alert(1)",
        ] {
            assert_eq!(buscar(url), None, "`{url}` atravessou");
        }
    }

    /// **O teto que o interpretador não dá.** Um endereço que aceita a conexão
    /// e nunca responde não pode segurar a fila de MODs para sempre.
    #[test]
    fn um_endereco_que_nunca_responde_e_cortado_pelo_teto() {
        let ouvinte = std::net::TcpListener::bind("127.0.0.1:0").expect("ouvir");
        let porta = ouvinte.local_addr().expect("endereço").port();

        // Aceita e cala. É o pior caso: a conexão abre, então nem o TCP
        // desiste sozinho.
        std::thread::spawn(move || {
            let _mudo: Vec<_> = ouvinte.incoming().take(1).collect();
            std::thread::sleep(std::time::Duration::from_secs(30));
        });

        let inicio = std::time::Instant::now();
        assert_eq!(buscar(&format!("http://127.0.0.1:{porta}/")), None);
        assert!(
            inicio.elapsed() < TETO_DA_BUSCA * 3,
            "a busca segurou a fila por {:?}",
            inicio.elapsed()
        );
    }

    /// **O teto corta enquanto os bytes chegam, e não depois de alocá-los.**
    ///
    /// R10 da revisão da v15.
    ///
    /// # O que este teste mede, e o que não bastaria medir
    ///
    /// «A busca devolveu `None`» **não** distingue as duas implementações: a de
    /// antes juntava o corpo inteiro e comparava depois, então ela também recusa.
    /// Medido, e é por isso que este parágrafo existe: revertendo para
    /// `resposta.text()` a asserção de `None` continuava passando.
    ///
    /// O que distingue é **quantos bytes o outro lado conseguiu empurrar**. Com o
    /// corte enquanto chega, quem lê para perto do teto e o servidor encontra o
    /// socket fechado; com o corte depois, quem lê aceita o fluxo inteiro e o
    /// servidor consegue escrever tudo. Por isso o servidor deste teste conta o
    /// que escreveu, e é sobre esse número que a asserção é feita.
    #[test]
    fn uma_resposta_grande_e_cortada_enquanto_chega() {
        use std::io::Write as _;
        use std::sync::atomic::{AtomicUsize, Ordering};
        use std::sync::Arc;

        let ouvinte = std::net::TcpListener::bind("127.0.0.1:0").expect("ouvir");
        let porta = ouvinte.local_addr().expect("endereço").port();
        /// Quanto o servidor de teste quer empurrar: bem acima do teto.
        const A_MANDAR: usize = 16 * 1024 * 1024;
        let escritos = Arc::new(AtomicUsize::new(0));
        let contando = Arc::clone(&escritos);

        std::thread::spawn(move || {
            let Ok((mut fluxo, _)) = ouvinte.accept() else {
                return;
            };
            ler_o_pedido(&mut fluxo);
            // **Sem `Content-Length`, e a primeira versão deste teste tinha um.**
            //
            // Ela declarava `Content-Length: 10` e enviava dezesseis megabytes —
            // o caso que o review pede, «resposta que declara tamanho menor do
            // que envia». Medido: o corpo voltou com **dez** bytes. Quem corta
            // ali é a camada HTTP do `reqwest`, respeitando o cabeçalho como o
            // HTTP manda, e não este teto. A anotação fica porque ela é a
            // resposta a «e se mentirem no tamanho?».
            //
            // O caso que **é** deste teto é o do tamanho desconhecido: sem
            // cabeçalho, quem lê vai até o fim do fluxo, e o fim é escolha de
            // quem está do outro lado.
            //
            // `concat!` e não um literal de várias linhas: num literal quebrado
            // a indentação entra nos bytes, e um cabeçalho HTTP com espaços no
            // começo da linha não é um cabeçalho HTTP.
            let cabecalho = concat!(
                "HTTP/1.1 200 OK\r\n",
                "Content-Type: text/plain\r\n",
                "Connection: close\r\n\r\n",
            );
            if fluxo.write_all(cabecalho.as_bytes()).is_err() {
                return;
            }
            let bloco = vec![b'a'; 64 * 1024];
            // Bem mais que o teto, e o laço para quando o outro lado fecha —
            // que é exatamente o que o corte faz.
            while contando.load(Ordering::Relaxed) < A_MANDAR {
                if fluxo.write_all(&bloco).is_err() {
                    return;
                }
                contando.fetch_add(bloco.len(), Ordering::Relaxed);
            }
        });

        assert_eq!(
            buscar(&format!("http://127.0.0.1:{porta}/")),
            None,
            "uma resposta acima do teto atravessou: o teto do QuickJS não alcança \
             esta alocação, e o prazo de busca não é um limite de bytes"
        );

        // O servidor de teste desiste quando o socket fecha; dar-lhe um instante
        // é o que separa «parou porque foi cortado» de «ainda não escreveu».
        std::thread::sleep(std::time::Duration::from_millis(300));
        let empurrados = escritos.load(Ordering::Relaxed);
        assert!(
            empurrados < A_MANDAR,
            "o outro lado conseguiu empurrar os {A_MANDAR} bytes inteiros: o corte \
             está acontecendo **depois** de o corpo ser alocado, e o teto do \
             QuickJS não alcança essa alocação (R10). Empurrados: {empurrados}"
        );
    }

    /// E uma resposta **dentro** do teto continua chegando inteira.
    ///
    /// Sem esta metade, um corte que recusasse tudo passaria no teste acima e
    /// desligaria a busca inteira sem deixar rastro — o mesmo par de asserções
    /// que os portões de versão desta casa têm.
    #[test]
    fn uma_resposta_pequena_chega_inteira() {
        use std::io::Write as _;

        let ouvinte = std::net::TcpListener::bind("127.0.0.1:0").expect("ouvir");
        let porta = ouvinte.local_addr().expect("endereço").port();

        std::thread::spawn(move || {
            let Ok((mut fluxo, _)) = ouvinte.accept() else {
                return;
            };
            ler_o_pedido(&mut fluxo);
            // **Sem `Content-Length`**, que é o outro caso que o review pede:
            // quem lê tem de parar no fim do fluxo, e não no número que não veio.
            //
            // `concat!` e não um literal quebrado em várias linhas: a indentação
            // entraria nos bytes, e um cabeçalho HTTP com espaços no começo da
            // linha não é um cabeçalho HTTP.
            let _ = fluxo.write_all(
                concat!(
                    "HTTP/1.1 200 OK\r\n",
                    "Content-Type: text/plain\r\n",
                    "Connection: close\r\n\r\n",
                    "padrao azul",
                )
                .as_bytes(),
            );
            // Fecha só a escrita, e espera. Largar o socket aqui pode chegar
            // como RST antes de o outro lado ler — ver `ler_o_pedido`.
            let _ = fluxo.shutdown(std::net::Shutdown::Write);
            std::thread::sleep(std::time::Duration::from_millis(200));
        });

        assert_eq!(
            buscar(&format!("http://127.0.0.1:{porta}/")).as_deref(),
            Some("padrao azul"),
            "uma resposta dentro do teto deixou de chegar: o corte passou a \
             recusar tudo, e a busca de MOD está desligada"
        );
    }

    #[test]
    fn o_relogio_anda_para_a_frente() {
        assert!(agora() > 1_700_000_000, "o relógio veio antes de 2023");
    }
}
