#!/usr/bin/env python3
"""O spike do executor de MODs no navegador: A (QuickJS em wasm num Worker),
B (iframe opaco com CSP sem rede) e AB (o motor num Worker de dentro do iframe).

    (cd motor && cargo build --release --target wasm32-wasip1)
    python3 prova.py chromium      # Chromium do Playwright, perfil persistente
    python3 prova.py safari-sim    # Safari do Simulador do iOS (um simulador ligado)
    python3 prova.py iphone        # só o custo, no iPhone, por HTTPS na LAN (o dono abre o endereço)

Fase 1, custo e MOD hostil gravando marcas; o navegador é FECHADO e aberto de
novo (no Simulador, o Safari é encerrado); fase 2, o MOD hostil procura as
marcas. Ninguém vai à rede de verdade: as canárias são servidores locais que
contam cada pedido (HTTP em 127.0.0.1:8096, UDP em 8097/8098/8099). Descartável.
"""
import base64, hashlib, http.server, json, pathlib, re, socket, subprocess, sys, tempfile, threading, time

AQUI = pathlib.Path(__file__).resolve().parent
PAGINA = AQUI / "pagina"
MOTOR = AQUI / "motor" / "target" / "wasm32-wasip1" / "release" / "motor_de_mods_wasm.wasm"
EXECUTOR = AQUI.parent.parent / "apps" / "seele-app" / "src" / "executor.rs"
PORTA, PORTA_CANARIO = 8095, 8096
UDP = {"A": 8097, "B": 8098, "AB": 8099}

# O prelúdio do desktop, lido do fonte, como o build.rs do motor faz.
fonte = EXECUTOR.read_text()
ini = fonte.index('const PRELUDIO: &str = r#"') + len('const PRELUDIO: &str = r#"')
PRELUDIO = fonte[ini:fonte.index('"#;', ini)]

# A CSP da página, no molde do ADR 0057: só scripts da origem, wasm permitido.
CSP_PAGINA = ("default-src 'self'; script-src 'self' 'wasm-unsafe-eval'; worker-src 'self'; frame-src 'self'; "
              "connect-src 'self'; img-src 'self'; style-src 'self' 'unsafe-inline'; frame-ancestors 'none'")
# A do Worker da candidata A: nada além do próprio script e do wasm. (O que o
# CSP não cobre é o IndexedDB da origem: é por isso que existe a AB.)
CSP_WORKER = "default-src 'none'; script-src 'self' 'wasm-unsafe-eval'"


def hash_do_script(html):
    corpo = re.search(r"<script>(.*?)</script>", html, re.S).group(1)
    return "'sha256-" + base64.b64encode(hashlib.sha256(corpo.encode()).digest()).decode() + "'"


def csp_do_iframe(arquivo):
    h = hash_do_script((PAGINA / arquivo).read_text())
    if arquivo == "candidato-ab.html":
        return f"default-src 'none'; script-src {h} 'wasm-unsafe-eval'; worker-src blob:"
    # `webrtc 'block'` é da CSP3, e é a única diretiva que fala de WebRTC:
    # sem ela, a CSP não cobre o STUN (medido: 8 pacotes STUN saíram do B).
    return f"default-src 'none'; script-src {h} 'unsafe-eval'; webrtc 'block'"


canarias, udp_vistos, resultados = [], [], {}
trava = threading.Lock()


def rss(nav):
    saida = subprocess.run(["ps", "-axo", "rss=,command="], capture_output=True, text=True).stdout
    total, processos = 0, 0
    for linha in saida.splitlines():
        kb, _, cmd = linha.strip().partition(" ")
        if nav.startswith("chromium") and "ms-playwright" in cmd:
            total += int(kb); processos += 1
        elif nav == "safari-sim" and "CoreSimulator" in cmd and "com.apple.WebKit.WebContent" in cmd:
            total += int(kb); processos += 1
    # Sem processo achado (o iPhone, por exemplo), não há medida: None, e não 0.
    return {"kb": total if processos else None, "processos": processos}


class Pagina(http.server.BaseHTTPRequestHandler):
    def responder(self, corpo, tipo, csp=None, status=200):
        self.send_response(status)
        self.send_header("content-type", tipo)
        self.send_header("cache-control", "no-store")
        if csp:
            self.send_header("content-security-policy", csp)
        self.end_headers()
        self.wfile.write(corpo)

    def do_GET(self):
        caminho = self.path.split("?")[0]
        if caminho.startswith("/canario/"):
            with trava:
                canarias.append({"origem": "mesma", "metodo": "GET", "caminho": caminho, "t": time.time()})
            return self.responder(b"", "text/plain")
        if caminho == "/rss":
            nav = re.search(r"nav=([^&]*)", self.path).group(1)
            return self.responder(json.dumps(rss(nav)).encode(), "application/json")
        if caminho == "/motor.wasm":
            return self.responder(MOTOR.read_bytes(), "application/wasm")
        if caminho == "/preludio.js":
            return self.responder(PRELUDIO.encode(), "text/plain; charset=utf-8")
        if caminho in ("/", "/index.html"):
            return self.responder((PAGINA / "index.html").read_bytes(), "text/html; charset=utf-8", CSP_PAGINA)
        nome = caminho.lstrip("/")
        arquivo = PAGINA / nome
        if not arquivo.is_file() or "/" in nome:
            return self.responder(b"nao", "text/plain", status=404)
        if nome.endswith(".html"):
            return self.responder(arquivo.read_bytes(), "text/html; charset=utf-8", csp_do_iframe(nome))
        csp = CSP_WORKER if nome == "candidato-quickjs-worker.js" else None
        return self.responder(arquivo.read_bytes(), "text/javascript; charset=utf-8", csp)

    def do_POST(self):
        n = int(self.headers.get("content-length", 0))
        corpo = self.rfile.read(n)
        if self.path.startswith("/canario/"):
            with trava:
                canarias.append({"origem": "mesma", "metodo": "POST", "caminho": self.path, "t": time.time()})
            return self.responder(b"", "text/plain")
        r = json.loads(corpo)
        with trava:
            resultados[(r["nav"], r["fase"])] = r
        self.responder(b"", "text/plain", status=204)

    def log_message(self, *a):
        pass


class Canario(http.server.BaseHTTPRequestHandler):
    """Outra origem, como o servidor de quem escreveu o MOD: aceita tudo e conta."""

    def tudo(self):
        with trava:
            canarias.append({"origem": "outra", "metodo": self.command, "caminho": self.path.split("?")[0],
                             "upgrade": self.headers.get("upgrade"), "t": time.time()})
        self.send_response(200)
        self.send_header("access-control-allow-origin", "*")
        self.send_header("content-type", "text/javascript" if self.path.endswith(".js") else "text/plain")
        self.end_headers()
        self.wfile.write(b"// canario\n")

    do_GET = do_POST = do_PUT = do_OPTIONS = do_HEAD = tudo

    def log_message(self, *a):
        pass


def escutar_udp(candidato, porta):
    s = socket.socket(socket.AF_INET, socket.SOCK_DGRAM)
    s.bind(("127.0.0.1", porta))
    while True:
        dados, de = s.recvfrom(65536)
        with trava:
            # STUN começa com o tipo 0x0001 (Binding Request) e o magic cookie
            # 0x2112A442 nos bytes 4 a 8; um QUIC Initial tem o bit alto ligado.
            stun = len(dados) >= 20 and dados[4:8] == bytes.fromhex("2112a442")
            udp_vistos.append({"candidato": candidato, "de": f"{de[0]}:{de[1]}", "bytes": len(dados),
                               "tipo": "STUN" if stun else ("QUIC" if dados[:1] and dados[0] & 0x80 else "?"), "t": time.time()})


def subir_servidores():
    for porta, classe in ((PORTA, Pagina), (PORTA_CANARIO, Canario)):
        h = http.server.ThreadingHTTPServer(("127.0.0.1", porta), classe)
        threading.Thread(target=h.serve_forever, daemon=True).start()
    for c, p in UDP.items():
        threading.Thread(target=escutar_udp, args=(c, p), daemon=True).start()


def esperar(nav, fase, prazo=420):
    fim = time.time() + prazo
    while time.time() < fim:
        with trava:
            if (nav, fase) in resultados:
                return resultados[(nav, fase)]
        time.sleep(0.5)
    return {"erro": f"a fase {fase} não voltou em {prazo} s"}


def rodar_chromium():
    from playwright.sync_api import sync_playwright
    perfil = tempfile.mkdtemp(prefix="mods-no-navegador-")
    fases = {}
    with sync_playwright() as p:
        for fase in (1, 2):
            ctx = p.chromium.launch_persistent_context(perfil)
            versao = ctx.browser.version if ctx.browser else "?"
            pg = ctx.pages[0]
            pg.goto(f"http://localhost:{PORTA}/?nav=chromium&fase={fase}")
            fases[fase] = esperar("chromium", fase)
            fases[fase]["navegador"] = versao
            ctx.close()  # o «app» fecha: a fase 2 é outra execução, com o mesmo perfil
    return fases


def rodar_safari_sim():
    fases = {}
    for fase in (1, 2):
        subprocess.run(["xcrun", "simctl", "openurl", "booted", f"http://localhost:{PORTA}/?nav=safari-sim&fase={fase}&t={int(time.time())}"], check=True)
        fases[fase] = esperar("safari-sim", fase)
        # O Safari é encerrado: a fase 2 é outra execução do app.
        subprocess.run(["xcrun", "simctl", "terminate", "booted", "com.apple.mobilesafari"], capture_output=True)
        time.sleep(2)
    return fases


def rodar_iphone():
    """Só o custo: a página por HTTPS na LAN (o WebAssembly e o Worker não pedem
    contexto seguro, mas a identidade de mentira da página usa o WebCrypto, que
    pede). As canárias ficam em 127.0.0.1 do Mac, fora do alcance do iPhone, e
    por isso o MOD hostil não roda aqui: ele mediria a rede do iPhone, e não o
    executor."""
    import ssl
    ip = subprocess.run(["ipconfig", "getifaddr", "en0"], capture_output=True, text=True).stdout.strip()
    pasta = AQUI / "resultados"
    pasta.mkdir(exist_ok=True)
    cert, chave = pasta / "pagina.pem", pasta / "pagina.key"
    if not cert.exists():
        subprocess.run(["openssl", "req", "-x509", "-newkey", "ec", "-pkeyopt", "ec_paramgen_curve:prime256v1",
                        "-pkeyopt", "ec_param_enc:named_curve", "-nodes", "-days", "7", "-subj", "/CN=seele-mods",
                        "-keyout", str(chave), "-out", str(cert)], check=True, capture_output=True)
    h = http.server.ThreadingHTTPServer(("0.0.0.0", 8443), Pagina)
    ctx = ssl.SSLContext(ssl.PROTOCOL_TLS_SERVER)
    ctx.load_cert_chain(cert, chave)
    h.socket = ctx.wrap_socket(h.socket, server_side=True)
    threading.Thread(target=h.serve_forever, daemon=True).start()
    print(f"\nNo iPhone, na mesma rede, abra:  https://{ip}:8443/?nav=iphone&fase=1&so=custo\n"
          "(o aviso de certificado é da página: «Mostrar detalhes → visitar este site»)\n", flush=True)
    return {1: esperar("iphone", 1, prazo=900)}


def main():
    nav = (sys.argv[1:] or ["chromium"])[0]
    if not MOTOR.exists():
        raise SystemExit("rode `cargo build --release --target wasm32-wasip1` em motor/ antes")
    subir_servidores()
    if nav == "iphone":
        fases = rodar_iphone()
        (AQUI / "resultados").mkdir(exist_ok=True)
        (AQUI / "resultados" / "iphone.json").write_text(json.dumps(fases, ensure_ascii=False, indent=1))
        print(json.dumps(fases, ensure_ascii=False, indent=1))
        return
    inicio = time.time()
    fases = rodar_chromium() if nav == "chromium" else rodar_safari_sim()
    time.sleep(3)  # um pedido atrasado ainda chega à canária
    with trava:
        hits = [c for c in canarias if c["t"] >= inicio]
        udp = [u for u in udp_vistos if u["t"] >= inicio]
    por_candidato = {}
    for c in hits:
        partes = c["caminho"].split("/")
        chave = partes[2] if len(partes) > 2 else "?"
        por_candidato.setdefault(chave, []).append(f'{c["origem"]} {c["metodo"]} {"/".join(partes[3:])}' + (f' (upgrade {c["upgrade"]})' if c.get("upgrade") else ""))
    saida = {"nav": nav, "fases": fases, "canarias_por_candidato": por_candidato,
             "udp_por_candidato": {k: [f'{u["tipo"]} {u["bytes"]} B de {u["de"]}' for u in udp if u["candidato"] == k] for k in UDP}}
    (AQUI / "resultados").mkdir(exist_ok=True)
    (AQUI / "resultados" / f"{nav}.json").write_text(json.dumps(saida, ensure_ascii=False, indent=1))
    print(json.dumps(saida, ensure_ascii=False, indent=1)[:200000])


if __name__ == "__main__":
    main()
