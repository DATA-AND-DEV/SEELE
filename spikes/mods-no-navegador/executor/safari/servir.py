# A bancada do executor no navegador para o Safari (Mac e Simulador): serve a
# ui/ copiada em /normal/ e em /fugido/ (com o Worker que saiu do motor), conta
# as canárias e recebe os resultados. Descartável.
import base64, hashlib, http.server, json, pathlib, re, subprocess, sys, threading, time
UI = pathlib.Path(sys.argv[1]); MOTOR = pathlib.Path(sys.argv[2]); MOD = pathlib.Path(sys.argv[3]); AQUI = pathlib.Path(__file__).parent
ALVO = sys.argv[4]
FUGIDO = open(AQUI / "fugido.js").read()
canarias, resultados = [], []
def iframe_com(anfitriao):
    h = (UI / "executor-de-mod.html").read_text()
    t = re.sub(r"  // <anfitrião>.*?// </anfitrião>\n", lambda m: "  function anfitriaoDoMotor() {\n" + anfitriao + "\n  }\n", h, count=1, flags=re.S)
    assert t != h
    corpo = t.split("<script>")[1].split("</script>")[0]
    return re.sub(r"'sha256-[^']*'", "'sha256-" + base64.b64encode(hashlib.sha256(corpo.encode()).digest()).decode() + "'", t, count=1)
CSP = "default-src 'self'; script-src 'self' 'wasm-unsafe-eval'; frame-ancestors 'none'"
class H(http.server.BaseHTTPRequestHandler):
    def log_message(self, *a): pass
    def responder(self, corpo, tipo, extra=None):
        self.send_response(200); self.send_header("content-type", tipo); self.send_header("cache-control", "no-store")
        for k, v in (extra or {}).items(): self.send_header(k, v)
        self.end_headers(); self.wfile.write(corpo)
    def do_GET(self):
        p = self.path.split("?")[0]
        if p.startswith("/canario"):
            canarias.append(f"GET {p} origin={self.headers.get('origin')}")
            self.send_response(204); self.send_header("access-control-allow-origin", "*"); self.end_headers(); return
        partes = p.strip("/").split("/", 1)
        if len(partes) != 2 or partes[0] not in ("normal", "fugido"): self.send_response(404); self.end_headers(); return
        modo, nome = partes
        if nome == "auto.html":
            return self.responder(b'<!doctype html><meta charset="utf-8"><title>bancada</title><script src="mods-runtime.js"></script><script src="mods-no-navegador.js"></script><script src="auto.js"></script>', "text/html", {"content-security-policy": CSP})
        if nome == "auto.js": return self.responder((AQUI / "auto.js").read_bytes(), "text/javascript")
        if nome == "mod-comum.js": return self.responder(MOD.read_bytes(), "text/javascript")
        if nome == "seele_motor_web.wasm": return self.responder(MOTOR.read_bytes(), "application/wasm")
        if nome == "executor-de-mod.html" and modo == "fugido": return self.responder(iframe_com(FUGIDO).encode(), "text/html", {"content-security-policy": "frame-ancestors 'self'"})
        f = UI / nome
        if not f.is_file(): self.send_response(404); self.end_headers(); return
        extra = {"content-security-policy": "frame-ancestors 'self'"} if nome == "executor-de-mod.html" else {}
        self.responder(f.read_bytes(), "text/html" if nome.endswith(".html") else "text/javascript", extra)
    def do_POST(self):
        n = int(self.headers.get("content-length", 0)); corpo = self.rfile.read(n)
        resultados.append(json.loads(corpo)); self.send_response(204); self.end_headers()
s = http.server.ThreadingHTTPServer(("127.0.0.1", int(sys.argv[5]) if len(sys.argv) > 5 else 0), H); porta = s.server_address[1]
threading.Thread(target=s.serve_forever, daemon=True).start()
for modo in ("normal", "fugido"):
    url = f"http://127.0.0.1:{porta}/{modo}/auto.html?t={time.time()}"
    antes = len(resultados); c0 = len(canarias)
    if ALVO == "sim": subprocess.run(["xcrun", "simctl", "openurl", "booted", url], check=True)
    else: subprocess.run(["open", "-a", "Safari", url], check=True)
    t0 = time.time()
    while len(resultados) == antes and time.time() - t0 < 60: time.sleep(0.2)
    time.sleep(2)
    r = resultados[-1] if len(resultados) > antes else {"caso": modo, "sem_resultado": True}
    r["canarias"] = canarias[c0:]
    print(json.dumps(r, ensure_ascii=False))
s.shutdown()
