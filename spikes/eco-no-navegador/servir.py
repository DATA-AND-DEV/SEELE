#!/usr/bin/env python3
"""A medida do eco no navegador: serve a página e guarda o que ela mede.

    python3 servir.py mac       # http://127.0.0.1:<porta>, para os navegadores do Mac
    python3 servir.py iphone    # https://<ip da LAN>:8443, para o iPhone na mesma rede
    python3 servir.py chromium  # o Chromium do Playwright, mudo e com microfone falso:
                                # só confere que a página roda do começo ao fim

Cada resultado vai para resultados/eco.jsonl, e o veredito é dito aqui. No
iPhone, o Safari avisa do certificado da página uma vez («Mostrar detalhes →
visitar este site»): o `getUserMedia` só existe em contexto seguro, e o
iPhone não alcança o localhost do Mac. Ctrl+C encerra.
"""
import http.server, json, pathlib, ssl, subprocess, sys, threading, time

AQUI = pathlib.Path(__file__).resolve().parent
SAIDA = AQUI / "resultados"
SAIDA.mkdir(exist_ok=True)
MODO = sys.argv[1] if len(sys.argv) > 1 else "mac"
if MODO not in ("mac", "iphone", "chromium"):
    raise SystemExit("uso: python3 servir.py mac|iphone|chromium")

PAGINA = ('<!doctype html><meta charset="utf-8">'
          '<meta name="viewport" content="width=device-width, initial-scale=1">'
          '<title>eco no navegador</title>'
          '<p>Sem fone, com o volume em uns dois terços. Aperte e fique em silêncio por uns 25 s.</p>'
          '<button id="medir" type="button">MEDIR</button><div id="saida"></div>'
          '<script src="pagina.js"></script>').encode()
recebidos = []


class Pagina(http.server.BaseHTTPRequestHandler):
    def log_message(self, *a):
        pass

    def do_GET(self):
        nome = self.path.split("?")[0].strip("/") or "index.html"
        if nome == "index.html":
            corpo, tipo = PAGINA, "text/html; charset=utf-8"
        elif nome == "pagina.js":
            corpo, tipo = (AQUI / "pagina.js").read_bytes(), "text/javascript; charset=utf-8"
        else:
            self.send_response(404)
            self.end_headers()
            return
        self.send_response(200)
        self.send_header("content-type", tipo)
        self.end_headers()
        self.wfile.write(corpo)

    def do_POST(self):
        corpo = json.loads(self.rfile.read(int(self.headers.get("content-length", 0))))
        corpo["recebido_em"] = time.strftime("%Y-%m-%dT%H:%M:%S")
        with (SAIDA / "eco.jsonl").open("a") as registro:
            registro.write(json.dumps(corpo, ensure_ascii=False) + "\n")
        recebidos.append(corpo)
        v = corpo.get("veredito")
        print(f"{corpo.get('agente', '?')[:90]}\n  -> {v['resposta'] if v else 'FALHOU: ' + corpo.get('falha', '?')}"
              + (f" (sem {v['retorno_sem_db']} dB, com {v['retorno_com_db']} dB)" if v else ""), flush=True)
        self.send_response(204)
        self.end_headers()


def ip_da_lan():
    for interface in ("en0", "en1"):
        r = subprocess.run(["ipconfig", "getifaddr", interface], capture_output=True, text=True)
        if r.stdout.strip():
            return r.stdout.strip()
    raise SystemExit("sem endereço na LAN (en0/en1)")


def certificado():
    cert, chave = SAIDA / "pagina.pem", SAIDA / "pagina.key"
    if not cert.exists():
        # Curva NOMEADA: a pilha TLS da Apple recusa os parâmetros explícitos
        # que o LibreSSL do macOS grava sem o ec_param_enc.
        subprocess.run(["openssl", "req", "-x509", "-newkey", "ec",
                        "-pkeyopt", "ec_paramgen_curve:prime256v1", "-pkeyopt", "ec_param_enc:named_curve",
                        "-nodes", "-days", "7", "-subj", "/CN=seele-eco",
                        "-keyout", str(chave), "-out", str(cert)], check=True, capture_output=True)
    return cert, chave


if MODO == "iphone":
    servidor = http.server.ThreadingHTTPServer(("0.0.0.0", 8443), Pagina)
    tls = ssl.SSLContext(ssl.PROTOCOL_TLS_SERVER)
    tls.load_cert_chain(*certificado())
    servidor.socket = tls.wrap_socket(servidor.socket, server_side=True)
    url = f"https://{ip_da_lan()}:8443/"
else:
    servidor = http.server.ThreadingHTTPServer(("127.0.0.1", 0), Pagina)
    url = f"http://127.0.0.1:{servidor.server_address[1]}/"
threading.Thread(target=servidor.serve_forever, daemon=True).start()

if MODO != "chromium":
    print(f"abra no navegador a medir: {url}", flush=True)
    try:
        while True:
            time.sleep(1)
    except KeyboardInterrupt:
        pass
    raise SystemExit(0)

# O Chromium do Playwright: mudo, com microfone falso e sem a caixa de
# permissão. Não mede eco nenhum (não há laço); confere que a página roda as
# quatro rodadas, que o cancelador pedido é o aplicado, e que o veredito diz
# «sem laço» em vez de inventar um número.
from playwright.sync_api import sync_playwright

with sync_playwright() as p:
    navegador = p.chromium.launch(args=["--mute-audio", "--use-fake-ui-for-media-stream",
                                        "--use-fake-device-for-media-stream"])
    aba = navegador.new_page()
    aba.goto(url)
    aba.click("#medir")
    prazo = time.time() + 60
    while not recebidos and time.time() < prazo:
        time.sleep(0.5)
    navegador.close()
if not recebidos:
    raise SystemExit("a página não mandou resultado em 60 s")
r = recebidos[0]
falhas = []
if "falha" in r:
    falhas.append(f"a página falhou: {r['falha']}")
else:
    if len(r["rodadas"]) != 4:
        falhas.append(f"{len(r['rodadas'])} rodadas, e não 4")
    for rodada in r["rodadas"]:
        if rodada["aplicado"] not in (rodada["pedido"], None):
            falhas.append(f"pedido {rodada['pedido']}, aplicado {rodada['aplicado']}")
    if not r["veredito"]["resposta"].startswith("sem laço"):
        falhas.append(f"sem laço de verdade, o veredito saiu «{r['veredito']['resposta']}»")
for f in falhas:
    print("FALHOU —", f)
raise SystemExit(1 if falhas else 0)
