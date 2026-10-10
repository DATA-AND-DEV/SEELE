#!/usr/bin/env python3
"""O wasm da voz dentro de um AudioWorklet, em navegadores de verdade.

    cargo build --release --target wasm32-unknown-unknown
    python3 servir.py chromium              # o Chromium do Playwright
    SEELE_SIMULADOR=<UDID> python3 servir.py simulador   # o Safari do Simulador
    python3 servir.py manual                # imprime o endereço

Descartável. O resultado sai aqui, em JSON.
"""
import http.server, json, os, pathlib, subprocess, sys, threading, time

AQUI = pathlib.Path(__file__).resolve().parent
WASM = AQUI / "target/wasm32-unknown-unknown/release/voz_em_wasm.wasm"
MODO = sys.argv[1] if len(sys.argv) > 1 else "manual"
resultados = []


class Pagina(http.server.BaseHTTPRequestHandler):
    def log_message(self, *a):
        pass

    def do_GET(self):
        nome = self.path.split("?")[0].strip("/") or "index.html"
        arquivos = {
            "index.html": (b'<!doctype html><meta charset="utf-8"><title>voz em wasm</title><script src="pagina.js"></script>', "text/html"),
            "pagina.js": ((AQUI / "pagina.js").read_bytes(), "text/javascript"),
            "worklet.js": ((AQUI / "worklet.js").read_bytes(), "text/javascript"),
            "voz_em_wasm.wasm": (WASM.read_bytes(), "application/wasm"),
        }
        if nome not in arquivos:
            self.send_response(404)
            self.end_headers()
            return
        corpo, tipo = arquivos[nome]
        self.send_response(200)
        self.send_header("content-type", tipo)
        self.send_header("cache-control", "no-store")
        self.end_headers()
        self.wfile.write(corpo)

    def do_POST(self):
        resultados.append(json.loads(self.rfile.read(int(self.headers.get("content-length", 0)))))
        self.send_response(204)
        self.end_headers()


servidor = http.server.ThreadingHTTPServer(("127.0.0.1", 0), Pagina)
threading.Thread(target=servidor.serve_forever, daemon=True).start()
url = f"http://127.0.0.1:{servidor.server_address[1]}/?t={time.time()}" + ("&via=bytes" if os.environ.get("VIA") == "bytes" else "")
if MODO == "chromium":
    from playwright.sync_api import sync_playwright
    with sync_playwright() as p:
        navegador = p.chromium.launch()
        aba = navegador.new_page()
        aba.goto(url)
        prazo = time.time() + 120
        while not resultados and time.time() < prazo:
            time.sleep(0.3)
        navegador.close()
elif MODO == "simulador":
    udid = os.environ.get("SEELE_SIMULADOR") or sys.exit("diga o Simulador: SEELE_SIMULADOR=<UDID>")
    subprocess.run(["xcrun", "simctl", "openurl", udid, url], check=True)
else:
    print(f"abra: {url}", flush=True)
prazo = time.time() + (600 if MODO == "manual" else 180)
while not resultados and time.time() < prazo:
    time.sleep(0.3)
print(json.dumps(resultados[0] if resultados else {"sem_resultado": True}, ensure_ascii=False, indent=1))
