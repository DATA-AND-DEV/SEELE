"""A medida da Tarefa 3.10: serve a página, recebe o resultado e os fluxos.

    python3 servir.py <saída> chromium|safari|manual

`manual` imprime o endereço e espera dez minutos: abra-o no navegador que
quiser medir (o Chrome, o Edge, o Firefox), nesta máquina.

Os fluxos ficam em <saída>/<navegador>-<codec>.bin (cada unidade de acesso com
o tamanho em 4 bytes na frente), para o OpenH264 do desktop decodificar.
"""
import http.server, json, pathlib, subprocess, sys, threading, time
AQUI = pathlib.Path(__file__).parent
SAIDA = pathlib.Path(sys.argv[1]); SAIDA.mkdir(parents=True, exist_ok=True)
NAV = sys.argv[2]
resultados = []
class H(http.server.BaseHTTPRequestHandler):
    def log_message(self, *a): pass
    def do_GET(self):
        nome = self.path.split("?")[0].strip("/") or "pagina.html"
        corpo = (b'<!doctype html><meta charset="utf-8"><script src="pagina.js"></script>' if nome == "pagina.html"
                 else (AQUI / nome).read_bytes() if (AQUI / nome).is_file() and nome == "pagina.js" else None)
        if corpo is None: self.send_response(404); self.end_headers(); return
        self.send_response(200); self.send_header("content-type", "text/html" if nome.endswith("html") else "text/javascript")
        self.end_headers(); self.wfile.write(corpo)
    def do_POST(self):
        corpo = self.rfile.read(int(self.headers.get("content-length", 0)))
        if self.path.startswith("/marco"):
            print("marco:", corpo.decode()[:600], flush=True)
        elif self.path.startswith("/fluxo"):
            codec = self.path.split("codec=")[1]
            (SAIDA / f"{NAV}-{codec}.bin").write_bytes(corpo)
        else:
            resultados.append(json.loads(corpo))
        self.send_response(204); self.end_headers()
s = http.server.ThreadingHTTPServer(("127.0.0.1", 0), H); porta = s.server_address[1]
threading.Thread(target=s.serve_forever, daemon=True).start()
url = f"http://127.0.0.1:{porta}/pagina.html?t={time.time()}"
if NAV == "manual":
    print(f"abra no navegador a medir: {url}", flush=True)
elif NAV == "safari":
    subprocess.run(["open", "-a", "Safari", url], check=True)
else:
    from playwright.sync_api import sync_playwright
    with sync_playwright() as p:
        b = p.chromium.launch(); pg = b.new_page(); pg.goto(url)
        t0 = time.time()
        while not resultados and time.time() - t0 < 90: time.sleep(0.3)
        b.close()
t0 = time.time()
while not resultados and time.time() - t0 < (600 if NAV == "manual" else 120): time.sleep(0.3)
s.shutdown()
print(json.dumps(resultados[0] if resultados else {"sem_resultado": True}, ensure_ascii=False))
