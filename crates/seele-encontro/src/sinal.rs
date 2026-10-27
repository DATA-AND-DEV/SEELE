//! O sinal do navegador (contrato da 0.17, D7): `POST /sinal/1`, uma linha de
//! texto de ida e outra de volta.
//!
//! O navegador não fala UDP, e o `CHAMA` dele chega por HTTPS. **O TLS não é
//! daqui.** Este módulo escuta HTTP/1.1 em claro, num endereço de loopback, e
//! um proxy reverso na mesma máquina (o Caddy de `docs/ponto-de-encontro.md`)
//! termina o TLS e repassa. Assim o ponto continua Rust puro, sem certificado e
//! sem ACME dentro dele.
//!
//! # O que este HTTP aceita
//!
//! - `POST /sinal/1`, corpo `text/plain` de até
//!   [`seele_proto::encontro::LIMITE_DO_SINAL`] bytes com o `Content-Length`
//!   dito: o `CHAMA` de [`seele_proto::encontro::pedido_do_sinal`]. A resposta é
//!   `200` com uma das quatro linhas de
//!   [`seele_proto::encontro::RespostaDoSinal`], que chega depois do `ATENDE`
//!   ou no fim de [`crate::enc3::PRAZO_DA_CHAMADA`];
//! - `OPTIONS /sinal/1`, a pergunta prévia do CORS, que o navegador só faz se
//!   a página mandar um cabeçalho que não é «simples» (ela não manda);
//! - o resto ouve `404`, `405`, `400` ou `413`, sem corpo útil.
//!
//! **O IP de quem liga**: o do par TCP, ou, quando o par é o loopback (o
//! proxy), o último do `X-Forwarded-For`, que é o que o proxy acrescentou. É
//! esse IP que gasta o balde de chamadas.
//!
//! **O CORS** devolve o `Access-Control-Allow-Origin` só para as origens
//! configuradas (`--origem`, padrão a origem do produto, [`origem_do_produto`]). Ele não protege
//! o ponto, porque qualquer um faz o `POST`, como qualquer um manda o `CHAMA`
//! por UDP. Ele impede que outra página **leia** a resposta, que diz se um
//! anfitrião está no ar e onde.

use std::io::{self, BufRead, BufReader, Read, Write};
use std::net::{IpAddr, SocketAddr, TcpListener, TcpStream};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::mpsc;
use std::sync::Arc;
use std::time::{Duration, Instant};

use seele_proto::encontro::{
    ler_pedido_do_sinal, resposta_do_sinal, sinal_de, Motivo, Resposta3, RespostaDoSinal,
    Vizinhanca, LIMITE_DO_SINAL,
};

use crate::enc3::{chamar, QuemLiga, PRAZO_DA_CHAMADA};
use crate::taxa::{Classe, Decisao};
use crate::Quarto;

/// A origem do web app do produto (`seele_proto::uri::ORIGEM_DO_NAVEGADOR`),
/// com o esquema: o padrão do `--origem`.
#[must_use]
pub fn origem_do_produto() -> String {
    format!("https://{}", seele_proto::uri::ORIGEM_DO_NAVEGADOR)
}

/// Quantos pedidos HTTP são atendidos ao mesmo tempo. Cada um espera no máximo
/// [`PRAZO_DA_CHAMADA`] pelo `ATENDE`; acima disto, `503`.
pub const PEDIDOS_AO_MESMO_TEMPO: usize = 256;

/// Quanto tempo o pedido tem para chegar inteiro.
const PRAZO_DA_LEITURA: Duration = Duration::from_secs(5);

/// O maior cabeçalho lido.
const LIMITE_DO_CABECALHO: usize = 8 * 1024;

/// O caminho do sinal.
pub const CAMINHO: &str = "/sinal/1";
/// Caixa assinada das chamadas pessoais, sem modificar o ENC/3.
pub const CAMINHO_DA_CHAMADA: &str = "/chamada/1";

/// O HTTP do sinal. Ver o cabeçalho do módulo.
#[derive(Debug)]
pub struct Sinal {
    escuta: TcpListener,
    quarto: Arc<Quarto>,
    vizinhanca: Vizinhanca,
    origens: Arc<Vec<String>>,
    abertos: Arc<AtomicUsize>,
    pessoais: Arc<crate::chamadas::ChamadasPessoais>,
}

impl Sinal {
    /// Abre a escuta TCP.
    ///
    /// # Errors
    ///
    /// Falha se a porta já estiver em uso.
    pub fn abrir(
        escuta: SocketAddr,
        quarto: Arc<Quarto>,
        vizinhanca: Vizinhanca,
        origens: Vec<String>,
    ) -> io::Result<Self> {
        Ok(Self {
            escuta: TcpListener::bind(escuta)?,
            quarto,
            vizinhanca,
            origens: Arc::new(origens),
            abertos: Arc::new(AtomicUsize::new(0)),
            pessoais: Arc::new(crate::chamadas::ChamadasPessoais::nova()?),
        })
    }

    /// Onde ele escuta.
    ///
    /// # Errors
    ///
    /// Falha se o sistema não disser.
    pub fn endereco(&self) -> io::Result<SocketAddr> {
        self.escuta.local_addr()
    }

    /// Atende para sempre: uma linha de execução por pedido, até
    /// [`PEDIDOS_AO_MESMO_TEMPO`].
    ///
    /// # Errors
    ///
    /// O erro de aceitação que não é de uma conexão só.
    pub fn servir(&self) -> io::Result<std::convert::Infallible> {
        loop {
            let (mut conexao, par) = match self.escuta.accept() {
                Ok(aceita) => aceita,
                Err(erro)
                    if matches!(
                        erro.kind(),
                        io::ErrorKind::ConnectionAborted
                            | io::ErrorKind::ConnectionReset
                            | io::ErrorKind::Interrupted
                    ) =>
                {
                    continue
                }
                Err(erro) => return Err(erro),
            };
            if self.abertos.fetch_add(1, Ordering::SeqCst) >= PEDIDOS_AO_MESMO_TEMPO {
                self.abertos.fetch_sub(1, Ordering::SeqCst);
                let _ = escrever(&mut conexao, 503, "Service Unavailable", &[], "");
                continue;
            }
            let pessoais = Arc::clone(&self.pessoais);
            let quarto = Arc::clone(&self.quarto);
            let origens = Arc::clone(&self.origens);
            let abertos = Arc::clone(&self.abertos);
            let vizinhanca = self.vizinhanca;
            std::thread::spawn(move || {
                let _ = atender(&mut conexao, par, &quarto, vizinhanca, &origens, &pessoais);
                abertos.fetch_sub(1, Ordering::SeqCst);
            });
        }
    }
}

/// Um pedido lido.
#[derive(Debug, Default)]
struct Pedido {
    metodo: String,
    caminho: String,
    origem: Option<String>,
    encaminhado: Option<String>,
    comprimento: Option<usize>,
    corpo: Vec<u8>,
}

// Cada leitura tem somente o tempo restante, inclusive um cabeçalho pingado.
struct LeituraComPrazo<'a> {
    conexao: &'a TcpStream,
    fim: Instant,
}
impl Read for LeituraComPrazo<'_> {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        let restante = self.fim.saturating_duration_since(Instant::now());
        if restante.is_zero() {
            return Err(io::ErrorKind::TimedOut.into());
        }
        self.conexao.set_read_timeout(Some(restante))?;
        self.conexao.read(buf)
    }
}

/// Lê o pedido, ou diz o código da recusa.
fn ler(conexao: &TcpStream) -> Result<Pedido, (u16, &'static str)> {
    conexao
        .set_read_timeout(Some(PRAZO_DA_LEITURA))
        .map_err(|_| (400, "Bad Request"))?;
    let mut leitor = BufReader::new(
        LeituraComPrazo {
            conexao,
            fim: Instant::now() + PRAZO_DA_LEITURA,
        }
        .take(
            u64::try_from(LIMITE_DO_CABECALHO + seele_proto::chamada::MAX_PEDIDO)
                .unwrap_or(u64::MAX),
        ),
    );
    let mut pedido = Pedido::default();
    let mut linha = String::new();
    let mut lidos = 0_usize;
    let mut primeira = true;
    loop {
        linha.clear();
        let n = leitor
            .read_line(&mut linha)
            .map_err(|_| (400, "Bad Request"))?;
        lidos += n;
        if n == 0 || lidos > LIMITE_DO_CABECALHO {
            return Err((400, "Bad Request"));
        }
        let limpa = linha.trim_end_matches(['\r', '\n']);
        if primeira {
            primeira = false;
            let mut partes = limpa.split(' ');
            pedido.metodo = partes.next().unwrap_or_default().to_owned();
            pedido.caminho = partes.next().unwrap_or_default().to_owned();
            if !partes
                .next()
                .is_some_and(|versao| versao.starts_with("HTTP/1."))
            {
                return Err((400, "Bad Request"));
            }
            continue;
        }
        if limpa.is_empty() {
            break;
        }
        let Some((nome, valor)) = limpa.split_once(':') else {
            return Err((400, "Bad Request"));
        };
        let valor = valor.trim();
        match nome.trim().to_ascii_lowercase().as_str() {
            "origin" => pedido.origem = Some(valor.to_owned()),
            "x-forwarded-for" => pedido.encaminhado = Some(valor.to_owned()),
            "content-length" => {
                if pedido.comprimento.is_some() {
                    return Err((400, "Bad Request"));
                }
                pedido.comprimento = Some(valor.parse().map_err(|_| (400, "Bad Request"))?);
            }
            "transfer-encoding" => return Err((400, "Bad Request")),
            _ => {}
        }
    }
    if pedido.metodo == "POST" {
        let comprimento = pedido.comprimento.ok_or((411, "Length Required"))?;
        let limite = if pedido.caminho == CAMINHO_DA_CHAMADA {
            seele_proto::chamada::MAX_PEDIDO
        } else {
            LIMITE_DO_SINAL
        };
        if comprimento > limite {
            return Err((413, "Content Too Large"));
        }
        let mut corpo = vec![0_u8; comprimento];
        leitor
            .read_exact(&mut corpo)
            .map_err(|_| (400, "Bad Request"))?;
        pedido.corpo = corpo;
    }
    Ok(pedido)
}

/// O IP de quem liga: o do par, ou o último do `X-Forwarded-For` quando o par
/// é o proxy no loopback.
fn ip_de_quem_liga(par: SocketAddr, encaminhado: Option<&str>) -> IpAddr {
    if par.ip().is_loopback() {
        if let Some(ip) = encaminhado
            .and_then(|lista| lista.rsplit(',').next())
            .and_then(|ultimo| ultimo.trim().parse::<IpAddr>().ok())
        {
            return ip;
        }
    }
    par.ip()
}

/// Os cabeçalhos do CORS para esta origem, se ela é uma das configuradas.
fn cors(origem: Option<&str>, origens: &[String]) -> Vec<(&'static str, String)> {
    let Some(origem) = origem else {
        return Vec::new();
    };
    if !origens
        .iter()
        .any(|permitida| permitida == "*" || permitida == origem)
    {
        return vec![("Vary", "Origin".to_owned())];
    }
    vec![
        ("Access-Control-Allow-Origin", origem.to_owned()),
        ("Vary", "Origin".to_owned()),
    ]
}

fn escrever(
    conexao: &mut TcpStream,
    codigo: u16,
    frase: &str,
    cabecalhos: &[(&'static str, String)],
    corpo: &str,
) -> io::Result<()> {
    let mut texto = format!(
        "HTTP/1.1 {codigo} {frase}\r\nContent-Type: text/plain; charset=utf-8\r\nCache-Control: no-store\r\nConnection: close\r\nContent-Length: {}\r\n",
        corpo.len()
    );
    for (nome, valor) in cabecalhos {
        texto.push_str(&format!("{nome}: {valor}\r\n"));
    }
    texto.push_str("\r\n");
    texto.push_str(corpo);
    let fim = Instant::now() + PRAZO_DA_LEITURA;
    let mut restante = texto.as_bytes();
    while !restante.is_empty() {
        let prazo = fim.saturating_duration_since(Instant::now());
        if prazo.is_zero() {
            return Err(io::ErrorKind::TimedOut.into());
        }
        conexao.set_write_timeout(Some(prazo))?;
        let escritos = conexao.write(restante)?;
        if escritos == 0 {
            return Err(io::ErrorKind::WriteZero.into());
        }
        restante = restante.get(escritos..).ok_or(io::ErrorKind::InvalidData)?;
    }
    conexao.flush()
}

fn atender(
    conexao: &mut TcpStream,
    par: SocketAddr,
    quarto: &Quarto,
    vizinhanca: Vizinhanca,
    origens: &[String],
    pessoais: &crate::chamadas::ChamadasPessoais,
) -> io::Result<()> {
    let pedido = match ler(conexao) {
        Ok(pedido) => pedido,
        Err((codigo, frase)) => return escrever(conexao, codigo, frase, &[], ""),
    };
    let mut cabecalhos = cors(pedido.origem.as_deref(), origens);
    if pedido.caminho == CAMINHO_DA_CHAMADA {
        return atender_pessoal(conexao, par, pedido, cabecalhos, pessoais);
    }
    if pedido.caminho != CAMINHO {
        return escrever(conexao, 404, "Not Found", &cabecalhos, "");
    }
    match pedido.metodo.as_str() {
        "OPTIONS" => {
            cabecalhos.push(("Access-Control-Allow-Methods", "POST".to_owned()));
            cabecalhos.push(("Access-Control-Allow-Headers", "Content-Type".to_owned()));
            cabecalhos.push(("Access-Control-Max-Age", "600".to_owned()));
            escrever(conexao, 204, "No Content", &cabecalhos, "")
        }
        "POST" => {
            let Some(chama) = std::str::from_utf8(&pedido.corpo)
                .ok()
                .and_then(ler_pedido_do_sinal)
            else {
                return escrever(conexao, 400, "Bad Request", &cabecalhos, "");
            };
            let ip = ip_de_quem_liga(par, pedido.encaminhado.as_deref());
            let resposta = responder(quarto, &chama, ip, vizinhanca);
            escrever(
                conexao,
                200,
                "OK",
                &cabecalhos,
                &resposta_do_sinal(&resposta),
            )
        }
        _ => {
            cabecalhos.push(("Allow", "POST, OPTIONS".to_owned()));
            escrever(conexao, 405, "Method Not Allowed", &cabecalhos, "")
        }
    }
}

/// A caixa pessoal não bloqueia a thread esperando uma chamada.
fn atender_pessoal(
    conexao: &mut TcpStream,
    par: SocketAddr,
    pedido: Pedido,
    mut cabecalhos: Vec<(&'static str, String)>,
    pessoais: &crate::chamadas::ChamadasPessoais,
) -> io::Result<()> {
    use seele_proto::chamada::{self, Motivo, Resposta};
    let ip = ip_de_quem_liga(par, pedido.encaminhado.as_deref());
    if !pessoais.admitir(ip, Instant::now()) {
        cabecalhos.push(("Retry-After", "1".into()));
        let corpo = chamada::escrever_resposta(&Resposta::Recusa {
            motivo: Motivo::Taxa,
        })
        .map_err(|_| io::Error::other("resposta da chamada invalida"))?;
        return escrever(conexao, 429, "Too Many Requests", &cabecalhos, &corpo);
    }
    let resposta = match pedido.metodo.as_str() {
        "OPTIONS" => {
            cabecalhos.push(("Access-Control-Allow-Methods", "GET, POST".into()));
            cabecalhos.push(("Access-Control-Allow-Headers", "Content-Type".into()));
            return escrever(conexao, 204, "No Content", &cabecalhos, "");
        }
        "GET" => Resposta::Desafio {
            epoca: pessoais.epoca(),
            agora: crate::agora_unix(),
        },
        "POST" => {
            let Some(p) = std::str::from_utf8(&pedido.corpo)
                .ok()
                .and_then(|s| chamada::ler_pedido(s).ok())
            else {
                return escrever(conexao, 400, "Bad Request", &cabecalhos, "");
            };
            pessoais
                .responder(&p, ip, Instant::now(), crate::agora_unix())
                .no_fio()
        }
        _ => {
            cabecalhos.push(("Allow", "GET, POST, OPTIONS".into()));
            return escrever(conexao, 405, "Method Not Allowed", &cabecalhos, "");
        }
    };
    let corpo = chamada::escrever_resposta(&resposta)
        .map_err(|_| io::Error::other("resposta da chamada invalida"))?;
    escrever(conexao, 200, "OK", &cabecalhos, &corpo)
}

/// A chamada do sinal, do balde à resposta.
fn responder(
    quarto: &Quarto,
    chama: &seele_proto::encontro::Chama,
    ip: IpAddr,
    vizinhanca: Vizinhanca,
) -> RespostaDoSinal {
    let agora = crate::agora_unix();
    // Pelo TCP não há origem forjada, e a recusa é sempre dita.
    if let Decisao::Recusa { .. } = quarto.taxa.pedir(ip, Classe::Chamada, Instant::now()) {
        return RespostaDoSinal::Recusa {
            motivo: Motivo::Taxa,
            agora,
        };
    }
    let (manda, espera) = mpsc::sync_channel::<Resposta3>(1);
    match chamar(
        quarto,
        chama,
        None,
        vizinhanca,
        QuemLiga::Sinal(manda),
        agora,
    ) {
        Err(caixa) => sinal_de(&caixa.1).unwrap_or(RespostaDoSinal::Calado),
        Ok(id) => {
            let resposta = espera.recv_timeout(PRAZO_DA_CHAMADA);
            quarto.chamadas.esquecer(&id);
            resposta
                .ok()
                .and_then(|resposta| sinal_de(&resposta))
                .unwrap_or(RespostaDoSinal::Calado)
        }
    }
}

#[cfg(test)]
mod testes {
    use super::*;

    #[test]
    fn o_ip_de_quem_liga_e_o_do_proxy_so_no_loopback() {
        let proxy: SocketAddr = "127.0.0.1:40000".parse().unwrap_or_else(|_| unreachable!());
        let fora: SocketAddr = "198.51.100.7:40000"
            .parse()
            .unwrap_or_else(|_| unreachable!());
        assert_eq!(
            ip_de_quem_liga(proxy, Some("10.0.0.1, 203.0.113.9")),
            "203.0.113.9"
                .parse::<IpAddr>()
                .unwrap_or_else(|_| unreachable!())
        );
        assert_eq!(ip_de_quem_liga(proxy, None), proxy.ip());
        assert_eq!(
            ip_de_quem_liga(fora, Some("203.0.113.9")),
            fora.ip(),
            "um par de fora não escolhe o próprio IP pelo cabeçalho"
        );
    }

    #[test]
    fn o_cors_so_abre_para_as_origens_configuradas() {
        let origens = vec![origem_do_produto()];
        let produto = origem_do_produto();
        assert!(cors(Some(&produto), &origens)
            .iter()
            .any(|(nome, valor)| *nome == "Access-Control-Allow-Origin" && *valor == produto));
        assert!(!cors(Some("https://outra.exemplo"), &origens)
            .iter()
            .any(|(nome, _)| *nome == "Access-Control-Allow-Origin"));
        assert!(cors(Some("http://localhost:1"), &["*".to_owned()])
            .iter()
            .any(|(nome, _)| *nome == "Access-Control-Allow-Origin"));
    }
}
