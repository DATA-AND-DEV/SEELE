//! Spike C (0.17): anfitrião str0m ICE-lite/DTLS passivo, e um cliente str0m
//! controlling, para medir as hipóteses do enlace contra navegadores reais.
//!
//! anfitriao [--porta P] [--http H] [--ip IP] [--sctp-max B] [--alto B] [--baixo B] [--lapide]
//! cliente --alvo IP:P --fp AA:BB:.. [--adulterar] [--furar-http H] [--n N]
//!
//! Ver README.md: os comandos, os navegadores e as limitações (só loopback).

mod cripto;

use std::collections::HashMap;
use std::io::{Read as _, Write as _};
use std::net::{IpAddr, SocketAddr, TcpListener, UdpSocket};
use std::sync::OnceLock;
use std::time::{Duration, Instant};

use str0m::channel::{ChannelConfig, ChannelId, Reliability};
use str0m::config::{DtlsCert, Fingerprint};
use str0m::ice::IceCreds;
use str0m::net::{Protocol, Receive};
use str0m::{Candidate, Event, IceConnectionState, Input, Output, Rtc, RtcConfig};

const PAGINA: &str = include_str!("../pagina.html");
const TETO_DE_PENDENTES: usize = 64;
static FP_DO_ANFITRIAO: OnceLock<String> = OnceLock::new();

fn t0() -> Instant {
    static T0: OnceLock<Instant> = OnceLock::new();
    *T0.get_or_init(Instant::now)
}

macro_rules! log {
    ($($arg:tt)*) => {{
        println!("[{:>9.3} ms] {}", t0().elapsed().as_secs_f64() * 1000.0, format!($($arg)*));
    }};
}

fn arg(args: &[String], nome: &str) -> Option<String> {
    args.iter().position(|a| a == nome).and_then(|i| args.get(i + 1).cloned())
}

fn main() {
    t0();
    let args: Vec<String> = std::env::args().collect();
    match args.get(1).map(String::as_str) {
        Some("anfitriao") => anfitriao(&args),
        Some("cliente") => {
            let ok = cliente(&args);
            std::process::exit(if ok { 0 } else { 1 });
        }
        _ => {
            eprintln!("uso: spike-rtc anfitriao|cliente ...");
            std::process::exit(2);
        }
    }
}

// ---- certificado ----------------------------------------------------------

/// O certificado do seeled de hoje: `rcgen::generate_simple_self_signed`
/// (crates/seele-server/src/tls.rs:54), P-256, validade padrão do rcgen.
fn certificado_do_seeled() -> (DtlsCert, Vec<u8>) {
    let gerado = rcgen::generate_simple_self_signed(vec!["localhost".to_string()])
        .expect("rcgen");
    let der = gerado.cert.der().to_vec();
    let chave = gerado.signing_key.serialize_der();
    (DtlsCert { certificate: der.clone(), private_key: chave }, der)
}

fn certificado_efemero() -> DtlsCert {
    certificado_do_seeled().0
}

fn sha256(bytes: &[u8]) -> [u8; 32] {
    use sha2::Digest;
    sha2::Sha256::digest(bytes).into()
}

fn hex_com_dois_pontos(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02X}")).collect::<Vec<_>>().join(":")
}

fn fp_de_texto(texto: &str) -> Fingerprint {
    let bytes = texto
        .split(':')
        .map(|h| u8::from_str_radix(h, 16).expect("hex"))
        .collect();
    Fingerprint { hash_func: "sha-256".into(), bytes }
}

// ---- STUN mínimo ----------------------------------------------------------

const COOKIE: u32 = 0x2112_A442;

/// O USERNAME de um Binding Request, se o pacote for um.
fn username_de_binding(pacote: &[u8]) -> Option<String> {
    if pacote.len() < 20 {
        return None;
    }
    let tipo = u16::from_be_bytes([pacote[0], pacote[1]]);
    let tamanho = u16::from_be_bytes([pacote[2], pacote[3]]) as usize;
    let cookie = u32::from_be_bytes([pacote[4], pacote[5], pacote[6], pacote[7]]);
    if tipo != 0x0001 || cookie != COOKIE || pacote.len() < 20 + tamanho {
        return None;
    }
    let mut i = 20;
    while i + 4 <= 20 + tamanho {
        let at = u16::from_be_bytes([pacote[i], pacote[i + 1]]);
        let n = u16::from_be_bytes([pacote[i + 2], pacote[i + 3]]) as usize;
        let corpo = pacote.get(i + 4..i + 4 + n)?;
        if at == 0x0006 {
            return String::from_utf8(corpo.to_vec()).ok();
        }
        i += 4 + n.div_ceil(4) * 4;
    }
    None
}

fn credencial_valida(u: &str) -> bool {
    u.len() == 32 && u.bytes().all(|b| b.is_ascii_alphanumeric())
}

fn crc32(dados: &[u8]) -> u32 {
    let mut crc = 0xFFFF_FFFFu32;
    for &b in dados {
        crc ^= u32::from(b);
        for _ in 0..8 {
            let mascara = (crc & 1).wrapping_neg();
            crc = (crc >> 1) ^ (0xEDB8_8320 & mascara);
        }
    }
    !crc
}

/// Binding Indication (0x0011) com FINGERPRINT, como o «furar» do D7.
fn binding_indication() -> Vec<u8> {
    let mut p = Vec::with_capacity(28);
    p.extend_from_slice(&0x0011u16.to_be_bytes());
    p.extend_from_slice(&8u16.to_be_bytes());
    p.extend_from_slice(&COOKIE.to_be_bytes());
    let txid = sha256(format!("{:?}", Instant::now()).as_bytes());
    p.extend_from_slice(&txid[..12]);
    let crc = crc32(&p) ^ 0x5354_554E;
    p.extend_from_slice(&0x8028u16.to_be_bytes());
    p.extend_from_slice(&4u16.to_be_bytes());
    p.extend_from_slice(&crc.to_be_bytes());
    p
}

fn sortear_u() -> String {
    const ALFABETO: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789";
    let semente = sha256(
        format!("{:?}{}{:?}", Instant::now(), std::process::id(), std::time::SystemTime::now())
            .as_bytes(),
    );
    let mais = sha256(&semente);
    semente
        .iter()
        .chain(mais.iter())
        .take(32)
        .map(|b| ALFABETO[usize::from(*b) % ALFABETO.len()] as char)
        .collect()
}

// ---- o anfitrião ----------------------------------------------------------

struct Rajada {
    cid: ChannelId,
    aberta: bool,
    total: usize,
    enviados: usize,
    inicio: Option<Instant>,
    recusas: u64,
    fim_enviado: bool,
}

struct Conexao {
    rtc: Rtc,
    u: String,
    criada: Instant,
    ctl: ChannelId,
    voz: ChannelId,
    extras: HashMap<ChannelId, String>,
    recebidos_u: HashMap<ChannelId, usize>,
    rajada: Option<Rajada>,
    voz_ecoada: u64,
    voz_recusada: u64,
    eco_recusado: u64,
    proximo: Instant,
    conectou: bool,
    /// STUN aceito por este Rtc desde o último relatório (consentimento).
    stun_no_segundo: u64,
    stun_total: u64,
    falhou: bool,
}

/// A correção do laço de ressurreição (pedido do coordenador).
const PRAZO_DA_LAPIDE: Duration = Duration::from_secs(30);
const PRAZO_DO_APERTO: Duration = Duration::from_secs(10);
const CRIACOES_POR_IP: usize = 4;
const JANELA_POR_IP: Duration = Duration::from_secs(10);

struct Anfitriao {
    socket: UdpSocket,
    local: SocketAddr,
    cert: DtlsCert,
    conexoes: Vec<Conexao>,
    sctp_max: usize,
    alto: usize,
    baixo: usize,
    /// `--lapide`: liga lápide do U, balde por IP, prazo de aperto e «um Rtc por U».
    lapide: bool,
    lapides: HashMap<String, Instant>,
    criacoes_por_ip: HashMap<IpAddr, Vec<Instant>>,
    criacoes_por_u: HashMap<String, u32>,
    criacoes_total: u64,
    recusas_lapide: u64,
    recusas_ip: u64,
    recusas_u_vivo: u64,
    recusas_no_segundo: u64,
    ultimo_relatorio: Instant,
}

fn anfitriao(args: &[String]) {
    let porta: u16 = arg(args, "--porta").map_or(0, |p| p.parse().expect("porta"));
    let http: u16 = arg(args, "--http").map_or(0, |p| p.parse().expect("http"));
    let ip: IpAddr = arg(args, "--ip").map_or("127.0.0.1".parse().unwrap(), |p| p.parse().expect("ip"));
    let sctp_max: usize = arg(args, "--sctp-max").map_or(1024 * 1024, |p| p.parse().expect("n"));
    let alto: usize = arg(args, "--alto").map_or(256 * 1024, |p| p.parse().expect("n"));
    let baixo: usize = arg(args, "--baixo").map_or(128 * 1024, |p| p.parse().expect("n"));

    let socket = UdpSocket::bind(SocketAddr::new("0.0.0.0".parse().unwrap(), porta)).expect("bind udp");
    let local = SocketAddr::new(ip, socket.local_addr().unwrap().port());
    let (cert, der) = certificado_do_seeled();
    let caminho_do_der = std::env::temp_dir().join("spike-webrtc-ice-lite-anfitriao.der");
    std::fs::write(&caminho_do_der, &der).ok();
    println!("DER={}", caminho_do_der.display());
    let fp = sha256(&der);
    println!("FP={}", hex_com_dois_pontos(&fp));
    let _ = FP_DO_ANFITRIAO.set(hex_com_dois_pontos(&fp));
    {
        use x509_cert::der::Decode;
        let c = x509_cert::Certificate::from_der(&der).expect("x509");
        let v = &c.tbs_certificate.validity;
        println!(
            "CERT validade {} .. {}; algoritmo da chave {}; assinatura {}; {} B",
            v.not_before,
            v.not_after,
            c.tbs_certificate.subject_public_key_info.algorithm.oid,
            c.signature_algorithm.oid,
            der.len()
        );
    }
    println!("UDP={local}");

    let tcp = TcpListener::bind(("127.0.0.1", http)).expect("bind http");
    println!("HTTP={}", tcp.local_addr().unwrap());
    println!("SCTP_MAX={sctp_max} ALTO={alto} BAIXO={baixo}");
    let furador = socket.try_clone().unwrap();
    std::thread::spawn(move || servir_http(tcp, furador));

    let lapide = args.iter().any(|a| a == "--lapide");
    println!("LAPIDE={lapide}");
    let mut a = Anfitriao {
        socket,
        local,
        cert,
        conexoes: Vec::new(),
        sctp_max,
        alto,
        baixo,
        lapide,
        lapides: HashMap::new(),
        criacoes_por_ip: HashMap::new(),
        criacoes_por_u: HashMap::new(),
        criacoes_total: 0,
        recusas_lapide: 0,
        recusas_ip: 0,
        recusas_u_vivo: 0,
        recusas_no_segundo: 0,
        ultimo_relatorio: Instant::now(),
    };
    a.rodar();
}

fn servir_http(tcp: TcpListener, socket: UdpSocket) {
    for fluxo in tcp.incoming() {
        let Ok(fluxo) = fluxo else { continue };
        // Uma linha por conexão: o WebKit abre conexões de pré-conexão que
        // nunca mandam nada, e um servidor sequencial ficava preso nelas.
        let socket = socket.try_clone().unwrap();
        std::thread::spawn(move || atender(fluxo, &socket));
    }
}

fn atender(mut fluxo: std::net::TcpStream, socket: &UdpSocket) {
    {
        let _ = fluxo.set_read_timeout(Some(Duration::from_secs(5)));
        let mut buf = [0u8; 4096];
        let n = fluxo.read(&mut buf).unwrap_or(0);
        if n == 0 {
            return;
        }
        let pedido = String::from_utf8_lossy(&buf[..n]).to_string();
        let caminho = pedido.split_whitespace().nth(1).unwrap_or("/").to_string();
        let (tipo, corpo) = if let Some(alvo) = caminho.strip_prefix("/furar?alvo=") {
            match alvo.parse::<SocketAddr>() {
                Ok(destino) => {
                    let pacote = binding_indication();
                    let r = socket.send_to(&pacote, destino);
                    log!("FURO: Binding Indication ({} B) para {destino}: {r:?}", pacote.len());
                    ("text/plain", format!("furado {destino}"))
                }
                Err(e) => ("text/plain", format!("alvo inválido {alvo:?}: {e}")),
            }
        } else if caminho == "/fp" {
            ("text/plain", FP_DO_ANFITRIAO.get().cloned().unwrap_or_default())
        } else if caminho.starts_with("/log?") {
            log!("PAGINA: {}", caminho);
            ("text/plain", "ok".into())
        } else {
            ("text/html; charset=utf-8", PAGINA.to_string())
        };
        let resposta = format!(
            "HTTP/1.1 200 OK\r\nContent-Type: {tipo}\r\nContent-Length: {}\r\nAccess-Control-Allow-Origin: *\r\nConnection: close\r\n\r\n{corpo}",
            corpo.len()
        );
        let _ = fluxo.write_all(resposta.as_bytes());
    }
}

impl Anfitriao {
    fn rodar(&mut self) {
        let mut buf = vec![0u8; 2000];
        loop {
            let agora = Instant::now();
            let proximo = self
                .conexoes
                .iter()
                .map(|c| c.proximo)
                .min()
                .unwrap_or(agora + Duration::from_millis(100));
            let espera = proximo
                .saturating_duration_since(agora)
                .clamp(Duration::from_millis(1), Duration::from_millis(100));
            self.socket.set_read_timeout(Some(espera)).unwrap();
            match self.socket.recv_from(&mut buf) {
                Ok((n, origem)) => self.chegou(&buf[..n].to_vec(), origem),
                Err(e) if matches!(e.kind(), std::io::ErrorKind::WouldBlock | std::io::ErrorKind::TimedOut) => {}
                Err(e) => log!("recv: {e}"),
            }
            let agora = Instant::now();
            for i in 0..self.conexoes.len() {
                if self.conexoes[i].proximo <= agora {
                    let c = &mut self.conexoes[i];
                    if let Err(e) = c.rtc.handle_input(Input::Timeout(agora)) {
                        log!("[{}] timeout erro: {e}", &c.u[..6]);
                        c.rtc.disconnect();
                    }
                }
                self.drenar(i);
                // Correção: prazo de aperto do enlace (o str0m não tem um).
                let c = &mut self.conexoes[i];
                if self.lapide && !c.conectou && c.rtc.is_alive() && c.criada.elapsed() > PRAZO_DO_APERTO {
                    log!("[{}] prazo de aperto ({PRAZO_DO_APERTO:?}) sem DTLS: derrubado", &c.u[..6]);
                    c.falhou = true;
                    c.rtc.disconnect();
                }
            }
            if self.ultimo_relatorio.elapsed() >= Duration::from_secs(1) {
                self.ultimo_relatorio = Instant::now();
                for c in &mut self.conexoes {
                    if c.stun_no_segundo > 0 {
                        log!(
                            "[{}] RELATORIO {} STUN/s aceitos (total {}), conectou={}, vivo há {:?}",
                            &c.u[..6], c.stun_no_segundo, c.stun_total, c.conectou, c.criada.elapsed()
                        );
                    }
                    c.stun_no_segundo = 0;
                }
                if self.recusas_no_segundo > 0 {
                    log!(
                        "RELATORIO {} Bindings recusados no último segundo (lápide {}, ip {}, U vivo {}); Rtc criados até agora {}",
                        self.recusas_no_segundo, self.recusas_lapide, self.recusas_ip, self.recusas_u_vivo, self.criacoes_total
                    );
                    self.recusas_no_segundo = 0;
                }
            }
            let antes = self.conexoes.len();
            let lapide = self.lapide;
            let mut mortas = Vec::new();
            self.conexoes.retain(|c| {
                let viva = c.rtc.is_alive();
                if !viva {
                    log!("[{}] conexão removida (viveu {:?})", &c.u[..6], c.criada.elapsed());
                    if lapide {
                        mortas.push(c.u.clone());
                    }
                }
                viva
            });
            for u in mortas {
                log!("[{}] lápide por {PRAZO_DA_LAPIDE:?}", &u[..6]);
                self.lapides.insert(u, Instant::now());
            }
            self.lapides.retain(|_, t| t.elapsed() < PRAZO_DA_LAPIDE);
            if antes != self.conexoes.len() {
                log!("conexões vivas: {}", self.conexoes.len());
            }
        }
    }

    fn chegou(&mut self, pacote: &[u8], origem: SocketAddr) {
        let classe = match pacote.first() {
            Some(0..=3) => "stun",
            Some(20..=63) => "dtls",
            Some(_) => "enc",
            None => return,
        };
        if classe == "enc" {
            log!("ENC de {origem}: {} B (não é WebRTC; iria ao ENC/3)", pacote.len());
            return;
        }
        let Ok(contents) = pacote.try_into() else {
            log!("pacote não reconhecido pelo str0m ({classe}) de {origem}");
            return;
        };
        let entrada = Input::Receive(
            Instant::now(),
            Receive { proto: Protocol::Udp, source: origem, destination: self.local, contents },
        );
        if let Some(i) = self.conexoes.iter().position(|c| c.rtc.accepts(&entrada)) {
            let c = &mut self.conexoes[i];
            if classe == "stun" {
                c.stun_no_segundo += 1;
                c.stun_total += 1;
            }
            if let Err(e) = c.rtc.handle_input(entrada) {
                log!("[{}] erro de entrada: {e}", &c.u[..6]);
                c.falhou = true;
                c.rtc.disconnect();
            }
            self.drenar(i);
            return;
        }
        // Ninguém aceitou: só um Binding com USERNAME U:U cria um Rtc.
        let Some(usuario) = username_de_binding(pacote) else {
            log!("{classe} de {origem} sem dono, descartado ({} B)", pacote.len());
            return;
        };
        let Some((a, b)) = usuario.split_once(':') else {
            log!("USERNAME sem ':' de {origem}: descartado");
            return;
        };
        if a != b || !credencial_valida(a) {
            log!("USERNAME {usuario:?} de {origem} não é U:U válido: descartado");
            return;
        }
        if self.conexoes.len() >= TETO_DE_PENDENTES {
            log!("teto de pendentes: descartado");
            return;
        }
        if self.lapide {
            // Nunca um segundo Rtc para um U vivo (inclusive um que acabou de falhar
            // e ainda não saiu do mapa).
            if self.conexoes.iter().any(|c| c.u == a) {
                self.recusas_u_vivo += 1;
                self.recusas_no_segundo += 1;
                return;
            }
            if self.lapides.get(a).is_some_and(|t| t.elapsed() < PRAZO_DA_LAPIDE) {
                self.recusas_lapide += 1;
                self.recusas_no_segundo += 1;
                return;
            }
            let fila = self.criacoes_por_ip.entry(origem.ip()).or_default();
            fila.retain(|t| t.elapsed() < JANELA_POR_IP);
            if fila.len() >= CRIACOES_POR_IP {
                self.recusas_ip += 1;
                self.recusas_no_segundo += 1;
                return;
            }
            fila.push(Instant::now());
        }
        self.criacoes_total += 1;
        let vez = {
            let n = self.criacoes_por_u.entry(a.to_string()).or_default();
            *n += 1;
            *n
        };
        log!("Binding de {origem} com U={}…: cria Rtc (vez {vez} deste U; {} no total)", &a[..6], self.criacoes_total);
        let mut c = self.nova_conexao(a.to_string());
        if let Err(e) = c.rtc.handle_input(entrada) {
            log!("erro no primeiro Binding: {e}");
            return;
        }
        self.conexoes.push(c);
        let i = self.conexoes.len() - 1;
        self.drenar(i);
    }

    fn nova_conexao(&self, u: String) -> Conexao {
        let creds = IceCreds { ufrag: u.clone(), pass: u.clone() };
        let mut rtc = RtcConfig::new()
            .set_crypto_provider(cripto::provedor())
            .set_dtls_cert(self.cert.clone())
            .set_local_ice_credentials(creds.clone())
            .set_ice_lite(true)
            .set_fingerprint_verification(false)
            .set_sctp_max_buffered_amount(self.sctp_max)
            .build(Instant::now());
        rtc.add_local_candidate(Candidate::host(self.local, "udp").unwrap());
        let (ctl, voz);
        {
            let mut d = rtc.direct_api();
            d.set_ice_controlling(false);
            d.set_remote_ice_credentials(creds);
            // (c) o str0m recusa o DTLS sem impressão remota («no a=fingerprint
            // before dtls», str0m lib.rs:1689): uma de enchimento, verificação desligada.
            d.set_remote_fingerprint(Fingerprint { hash_func: "sha-256".into(), bytes: vec![0; 32] });
            d.start_dtls(false).unwrap();
            d.start_sctp(false);
            ctl = d.create_data_channel(ChannelConfig {
                label: "ctl".into(),
                ordered: true,
                reliability: Reliability::Reliable,
                negotiated: Some(0),
                protocol: String::new(),
            });
            voz = d.create_data_channel(ChannelConfig {
                label: "voz".into(),
                ordered: false,
                reliability: Reliability::MaxRetransmits { retransmits: 0 },
                negotiated: Some(1),
                protocol: String::new(),
            });
        }
        Conexao {
            rtc,
            u,
            criada: Instant::now(),
            ctl,
            voz,
            extras: HashMap::new(),
            recebidos_u: HashMap::new(),
            rajada: None,
            voz_ecoada: 0,
            voz_recusada: 0,
            eco_recusado: 0,
            proximo: Instant::now(),
            conectou: false,
            stun_no_segundo: 1,
            stun_total: 1,
            falhou: false,
        }
    }

    fn drenar(&mut self, i: usize) {
        let (alto, baixo) = (self.alto, self.baixo);
        loop {
            let c = &mut self.conexoes[i];
            bombear(c, alto);
            let saida = match c.rtc.poll_output() {
                Ok(s) => s,
                Err(e) => {
                    log!("[{}] poll erro: {e}", &c.u[..6]);
                    c.rtc.disconnect();
                    return;
                }
            };
            match saida {
                Output::Timeout(t) => {
                    c.proximo = t;
                    return;
                }
                Output::Transmit(t) => {
                    if let Err(e) = self.socket.send_to(&t.contents, t.destination) {
                        log!("send_to {}: {e}", t.destination);
                    }
                }
                Output::Event(ev) => evento(c, ev, baixo),
            }
        }
    }
}

fn evento(c: &mut Conexao, ev: Event, baixo: usize) {
    let tag = c.u[..6].to_string();
    match ev {
        Event::IceConnectionStateChange(s) => log!("[{tag}] ICE {s:?}"),
        Event::Connected => {
            c.conectou = true;
            let fp_par = c.rtc.direct_api().remote_dtls_fingerprint().cloned();
            let versao = c.rtc.direct_api().dtls_protocol_version();
            log!(
                "[{tag}] DTLS conectado em {:?} desde o 1º Binding; versão {versao:?}; impressão do par (lida do certificado) = {}",
                c.criada.elapsed(),
                fp_par.map_or("?".into(), |f| hex_com_dois_pontos(&f.bytes))
            );
        }
        Event::ChannelOpen(cid, label) => {
            let (proto, sid) = {
                let ch = c.rtc.channel(cid);
                let proto = ch.as_ref().and_then(|ch| ch.config().map(|k| k.protocol.clone()));
                (proto, c.rtc.direct_api().sctp_stream_id_by_channel_id(cid))
            };
            log!("[{tag}] canal aberto {cid:?} sid={sid:?} label={label:?} protocol={proto:?} em {:?}", c.criada.elapsed());
            if cid != c.ctl && cid != c.voz {
                if label != "u" || proto.as_deref() != Some("seele/2") {
                    log!("[{tag}] canal extra fora do contrato: fechado");
                    c.rtc.direct_api().close_data_channel(cid);
                    return;
                }
                c.extras.insert(cid, label);
                if let Some(r) = c.rajada.as_mut() {
                    if r.cid == cid {
                        r.aberta = true;
                        r.inicio = Some(Instant::now());
                        if let Some(mut ch) = c.rtc.channel(cid) {
                            ch.set_buffered_amount_low_threshold(baixo);
                        }
                    }
                }
            }
        }
        Event::ChannelData(d) => dados(c, d.id, d.binary, d.data),
        Event::ChannelBufferedAmountLow(_) => {}
        Event::ChannelClose(cid) => log!("[{tag}] canal fechado {cid:?}"),
        Event::Closed => log!("[{tag}] DTLS close_notify"),
        _ => {}
    }
}

fn escrever(c: &mut Conexao, cid: ChannelId, binario: bool, bytes: &[u8]) -> bool {
    match c.rtc.channel(cid) {
        Some(mut ch) => ch.write(binario, bytes).unwrap_or(false),
        None => false,
    }
}

fn dados(c: &mut Conexao, cid: ChannelId, binario: bool, data: Vec<u8>) {
    let tag = c.u[..6].to_string();
    if cid == c.voz {
        if escrever(c, cid, binario, &data) {
            c.voz_ecoada += 1;
        } else {
            c.voz_recusada += 1;
        }
        return;
    }
    if cid == c.ctl {
        if !binario {
            if let Ok(texto) = std::str::from_utf8(&data) {
                if let Some(n) = texto.strip_prefix("!DESCE ") {
                    let total: usize = n.trim().parse().unwrap_or(0);
                    let rid = c.rtc.direct_api().create_data_channel(ChannelConfig {
                        label: "u".into(),
                        ordered: true,
                        reliability: Reliability::Reliable,
                        negotiated: None,
                        protocol: "seele/2".into(),
                    });
                    log!("[{tag}] rajada para baixo: {total} B num DCEP «u» aberto pelo anfitrião ({rid:?})");
                    c.rajada = Some(Rajada { cid: rid, aberta: false, total, enviados: 0, inicio: None, recusas: 0, fim_enviado: false });
                    return;
                }
                if texto == "!STATS" {
                    let r = c.rajada.as_ref();
                    let json = format!(
                        "{{\"voz_ecoada\":{},\"voz_recusada\":{},\"eco_recusado\":{},\"rajada_enviados\":{},\"rajada_recusas\":{},\"recebidos_u\":{:?}}}",
                        c.voz_ecoada,
                        c.voz_recusada,
                        c.eco_recusado,
                        r.map_or(0, |r| r.enviados),
                        r.map_or(0, |r| r.recusas),
                        c.recebidos_u.values().collect::<Vec<_>>()
                    );
                    log!("[{tag}] STATS {json}");
                    escrever(c, cid, false, json.as_bytes());
                    return;
                }
            }
        }
        if !escrever(c, cid, binario, &data) {
            c.eco_recusado += 1;
        }
        return;
    }
    // Canal extra «u».
    match data.first() {
        Some(b'E') => {
            if !escrever(c, cid, binario, &data) {
                c.eco_recusado += 1;
            }
        }
        Some(0x00) => {
            *c.recebidos_u.entry(cid).or_default() += data.len() - 1;
        }
        _ if data == b"FIM" => {
            let n = c.recebidos_u.get(&cid).copied().unwrap_or(0);
            log!("[{tag}] rajada para cima terminou: {n} B de dados no {cid:?}");
            escrever(c, cid, false, format!("TOTAL {n}").as_bytes());
        }
        _ => {}
    }
}

/// Escreve a rajada para baixo enquanto o canal tem menos que `alto` no buffer.
fn bombear(c: &mut Conexao, alto: usize) {
    let Some(r) = c.rajada.as_mut() else { return };
    if !r.aberta || r.fim_enviado {
        return;
    }
    let cid = r.cid;
    let mut pedaco = vec![0u8; 16385];
    loop {
        let r = c.rajada.as_mut().unwrap();
        if r.enviados >= r.total {
            break;
        }
        let Some(mut ch) = c.rtc.channel(cid) else { return };
        if ch.buffered_amount() >= alto {
            return;
        }
        let n = (r.total - r.enviados).min(16384);
        pedaco[0] = 0x00;
        let ok = ch.write(true, &pedaco[..n + 1]).unwrap_or(false);
        if !ok {
            r.recusas += 1;
            return;
        }
        r.enviados += n;
    }
    let r = c.rajada.as_mut().unwrap();
    let inicio = r.inicio.unwrap_or_else(Instant::now);
    let total = r.total;
    if let Some(mut ch) = c.rtc.channel(cid) {
        if ch.write(false, b"FIM").unwrap_or(false) {
            r.fim_enviado = true;
            log!("[{}] rajada para baixo entregue ao SCTP: {total} B em {:?}", &c.u[..6], inicio.elapsed());
        }
    }
}

// ---- o cliente nativo (controlling) ---------------------------------------

fn cliente(args: &[String]) -> bool {
    let alvo: SocketAddr = arg(args, "--alvo").expect("--alvo").parse().expect("alvo");
    let mut fp = arg(args, "--fp").expect("--fp");
    let adulterar = args.iter().any(|a| a == "--adulterar");
    let furar_http = arg(args, "--furar-http");
    let n: usize = arg(args, "--n").map_or(100, |p| p.parse().expect("n"));
    if adulterar {
        // troca o primeiro byte
        let resto = fp[2..].to_string();
        let b = u8::from_str_radix(&fp[..2], 16).unwrap() ^ 0xFF;
        fp = format!("{b:02X}{resto}");
    }
    let socket = UdpSocket::bind("127.0.0.1:0").unwrap();
    let local = socket.local_addr().unwrap();
    if let Some(h) = furar_http {
        let mut s = std::net::TcpStream::connect(("127.0.0.1", h.parse::<u16>().unwrap())).unwrap();
        write!(s, "GET /furar?alvo={local} HTTP/1.0\r\n\r\n").unwrap();
        let mut r = String::new();
        let _ = s.read_to_string(&mut r);
        log!("pedido de furo: {}", r.lines().last().unwrap_or(""));
        // o Binding Indication chega antes dos checks; o str0m deve ignorá-lo
    }
    let u = sortear_u();
    let creds = IceCreds { ufrag: u.clone(), pass: u.clone() };
    let meu = certificado_efemero();
    log!("cliente U={}… impressão DTLS própria (efêmera) = {}", &u[..6], hex_com_dois_pontos(&sha256(&meu.certificate)));
    let mut rtc = RtcConfig::new()
        .set_crypto_provider(cripto::provedor())
        .set_dtls_cert(meu)
        .set_local_ice_credentials(creds.clone())
        .build(Instant::now());
    rtc.add_local_candidate(Candidate::host(local, "udp").unwrap());
    rtc.add_remote_candidate(Candidate::host(alvo, "udp").unwrap());
    let (ctl, voz);
    let mut u_canal: Option<ChannelId> = None;
    {
        let mut d = rtc.direct_api();
        d.set_ice_controlling(true);
        d.set_remote_ice_credentials(creds);
        d.set_remote_fingerprint(fp_de_texto(&fp));
        d.start_dtls(true).unwrap();
        d.start_sctp(true);
        ctl = d.create_data_channel(ChannelConfig { label: "ctl".into(), ordered: true, reliability: Reliability::Reliable, negotiated: Some(0), protocol: String::new() });
        voz = d.create_data_channel(ChannelConfig { label: "voz".into(), ordered: false, reliability: Reliability::MaxRetransmits { retransmits: 0 }, negotiated: Some(1), protocol: String::new() });
    }
    let inicio = Instant::now();
    let mut abertos: HashMap<ChannelId, Instant> = HashMap::new();
    let mut recebidos: HashMap<ChannelId, Vec<usize>> = HashMap::new();
    let mut enviados = false;
    let mut buf = vec![0u8; 2000];
    let mut furo_visto = false;
    loop {
        if inicio.elapsed() > Duration::from_secs(10) {
            log!("PRAZO de 10 s estourado");
            break;
        }
        let espera = loop {
            match rtc.poll_output() {
                Ok(Output::Timeout(t)) => break t,
                Ok(Output::Transmit(t)) => {
                    let _ = socket.send_to(&t.contents, t.destination);
                }
                Ok(Output::Event(ev)) => match ev {
                    Event::IceConnectionStateChange(s) => {
                        log!("cliente ICE {s:?}");
                        if s == IceConnectionState::Disconnected {
                            log!("cliente: ICE desconectou");
                        }
                    }
                    Event::Connected => log!(
                        "cliente DTLS conectado em {:?}; versão {:?}",
                        inicio.elapsed(),
                        rtc.direct_api().dtls_protocol_version()
                    ),
                    Event::ChannelOpen(cid, label) => {
                        log!("cliente canal aberto {cid:?} {label:?} em {:?}", inicio.elapsed());
                        abertos.insert(cid, Instant::now());
                        if cid == voz {
                            u_canal = Some(rtc.direct_api().create_data_channel(ChannelConfig {
                                label: "u".into(),
                                ordered: true,
                                reliability: Reliability::Reliable,
                                negotiated: None,
                                protocol: "seele/2".into(),
                            }));
                        }
                    }
                    Event::ChannelData(d) => {
                        let texto = String::from_utf8_lossy(&d.data);
                        if let Some(seq) = texto.split(':').nth(1).and_then(|s| s.parse::<usize>().ok()) {
                            recebidos.entry(d.id).or_default().push(seq);
                        }
                    }
                    _ => {}
                },
                Err(e) => {
                    log!("cliente RECUSOU: {e} (em {:?})", inicio.elapsed());
                    return adulterar;
                }
            }
        };
        if !enviados {
            if let Some(uc) = u_canal {
                if abertos.contains_key(&ctl) && abertos.contains_key(&voz) && abertos.contains_key(&uc) {
                    for (cid, nome) in [(ctl, "ctl"), (voz, "voz"), (uc, "u")] {
                        for i in 0..n {
                            let msg = format!("E{nome}:{i}");
                            if let Some(mut ch) = rtc.channel(cid) {
                                if !ch.write(false, msg.as_bytes()).unwrap_or(false) {
                                    log!("escrita recusada {nome} {i}");
                                }
                            }
                        }
                    }
                    enviados = true;
                    log!("cliente mandou {n} em cada canal");
                }
            }
        }
        if enviados {
            let completos = [ctl, voz, u_canal.unwrap()]
                .iter()
                .all(|cid| recebidos.get(cid).map_or(0, Vec::len) >= n);
            if completos {
                break;
            }
        }
        let agora = Instant::now();
        let dur = espera.saturating_duration_since(agora).clamp(Duration::from_millis(1), Duration::from_millis(50));
        socket.set_read_timeout(Some(dur)).unwrap();
        match socket.recv_from(&mut buf) {
            Ok((tam, origem)) => {
                if buf[0] < 4 && tam >= 2 && u16::from_be_bytes([buf[0], buf[1]]) == 0x0011 {
                    log!("cliente viu Binding Indication de {origem} ({tam} B) antes/durante os checks");
                    furo_visto = true;
                }
                let Ok(contents) = buf[..tam].try_into() else { continue };
                let r = Receive { proto: Protocol::Udp, source: origem, destination: local, contents };
                if let Err(e) = rtc.handle_input(Input::Receive(Instant::now(), r)) {
                    log!("cliente RECUSOU na entrada: {e} (em {:?})", inicio.elapsed());
                    return adulterar;
                }
            }
            Err(_) => {
                if let Err(e) = rtc.handle_input(Input::Timeout(Instant::now())) {
                    log!("cliente erro timeout: {e}");
                    return adulterar;
                }
            }
        }
    }
    let _ = furo_visto;
    let mut ok = !adulterar;
    for (cid, nome) in [(ctl, "ctl"), (voz, "voz")].into_iter().chain(u_canal.map(|c| (c, "u"))) {
        let v = recebidos.get(&cid).cloned().unwrap_or_default();
        let em_ordem = v.windows(2).all(|w| w[0] < w[1]);
        let mut unicos = v.clone();
        unicos.sort_unstable();
        unicos.dedup();
        let perdas = n.saturating_sub(unicos.len());
        log!("RESULTADO {nome}: {} de {n} ecoadas, perdas={perdas}, em ordem={em_ordem}", v.len());
        if perdas > 0 || (nome != "voz" && !em_ordem) {
            ok = false;
        }
    }
    log!("total {:?}", inicio.elapsed());
    ok
}
