#!/usr/bin/env python3
"""A medição 3 no iPhone: o dono abre a página no aparelho, na mesma rede do Mac.

    cargo build --release
    python3 iphone.py

Sobe o binário escutando na LAN (portas UDP 4500 a 4504) e serve a página por
HTTPS na porta 8443, com um certificado autoassinado de curva nomeada. O
WebTransport só existe em contexto seguro, e o iPhone não alcança o localhost
do Mac. O Safari avisa do certificado da PÁGINA uma vez («Mostrar detalhes →
visitar este site»). O WebTransport entra pelo hash e não depende dessa
exceção, e é isso que se mede.

Tudo o que a página manda vai para resultados/iphone.jsonl, e tudo o que o
servidor registra vai para resultados/servidor-iphone.jsonl. Ctrl+C encerra.
Descartável.
"""
import http.server, json, pathlib, ssl, subprocess, sys, threading, time

AQUI = pathlib.Path(__file__).resolve().parent
BIN = AQUI / "target" / "release" / "alpn-demux"
PORTA_PAGINA = 8443
SAIDA = AQUI / "resultados"
SAIDA.mkdir(exist_ok=True)


def ip_da_lan():
    for interface in ("en0", "en1"):
        r = subprocess.run(["ipconfig", "getifaddr", interface], capture_output=True, text=True)
        if r.stdout.strip():
            return r.stdout.strip()
    raise SystemExit("sem endereço na LAN (en0/en1)")


def certificado_da_pagina():
    cert, chave = SAIDA / "pagina.pem", SAIDA / "pagina.key"
    if not cert.exists():
        # Curva NOMEADA: o LibreSSL do macOS grava parâmetros explícitos sem o
        # ec_param_enc, e a pilha TLS da Apple recusa com «decode error».
        subprocess.run(["openssl", "req", "-x509", "-newkey", "ec",
                        "-pkeyopt", "ec_paramgen_curve:prime256v1", "-pkeyopt", "ec_param_enc:named_curve",
                        "-nodes", "-days", "7", "-subj", "/CN=seele-medicao-3",
                        "-keyout", str(chave), "-out", str(cert)], check=True, capture_output=True)
    return cert, chave


if not BIN.exists():
    raise SystemExit("rode `cargo build --release` antes")
host = ip_da_lan()
srv = subprocess.Popen([str(BIN), "--lan", "--porta-base", "4500"], stdout=subprocess.PIPE, stderr=subprocess.PIPE, text=True)
cfg = json.loads(srv.stdout.readline())
cfg["host"] = host
registro_servidor = (SAIDA / "servidor-iphone.jsonl").open("a")
registro_pagina = (SAIDA / "iphone.jsonl").open("a")


def ler_stderr():
    for linha in srv.stderr:
        registro_servidor.write(linha)
        registro_servidor.flush()
        try:
            e = json.loads(linha)
        except json.JSONDecodeError:
            continue
        if e.get("ev") in ("aberta", "fechou", "falhou", "mudo_primeiro_pacote"):
            print(f"[servidor {time.strftime('%H:%M:%S')}] {e.get('ev')} {e.get('ponto', '')} {e.get('de', '')} "
                  f"{('durou ' + str(e['durou_ms']) + ' ms · ' + e['motivo']) if e.get('ev') == 'fechou' else ''}"
                  f"{e.get('erro', '')[:120] if e.get('ev') == 'falhou' else ''}", flush=True)


threading.Thread(target=ler_stderr, daemon=True).start()


class H(http.server.BaseHTTPRequestHandler):
    def do_GET(self):
        caminho = self.path.split("?")[0]
        if caminho == "/cfg":
            corpo, tipo = json.dumps(cfg).encode(), "application/json"
        elif caminho == "/casos.js":
            corpo, tipo = (AQUI / "casos.js").read_bytes(), "text/javascript; charset=utf-8"
        else:
            corpo, tipo = (AQUI / "pagina-iphone.html").read_bytes(), "text/html; charset=utf-8"
        self.send_response(200)
        self.send_header("content-type", tipo)
        self.send_header("cache-control", "no-store")
        self.end_headers()
        self.wfile.write(corpo)

    def do_POST(self):
        n = int(self.headers.get("content-length", 0))
        corpo = json.loads(self.rfile.read(n))
        corpo["recebido_em"] = time.strftime("%Y-%m-%d %H:%M:%S")
        corpo["tipo"] = "evento" if self.path.startswith("/evento") else "resultado"
        registro_pagina.write(json.dumps(corpo, ensure_ascii=False) + "\n")
        registro_pagina.flush()
        if corpo["tipo"] == "resultado":
            print(f"\n[página] {corpo.get('caso')}:", flush=True)
            for c in corpo.get("resultados") or []:
                estado = f"ABRIU {c.get('ms')} ms" if c.get("abriu") else f"não abriu {c.get('ms')} ms · {c.get('erro')}"
                print(f"   {c.get('rotulo')}: {estado}", flush=True)
        else:
            print(f"[página {time.strftime('%H:%M:%S')}] {json.dumps({k: v for k, v in corpo.items() if k not in ('ua', 'recebido_em', 'tipo')}, ensure_ascii=False)}", flush=True)
        self.send_response(204)
        self.end_headers()

    def log_message(self, *a):
        pass


httpd = http.server.ThreadingHTTPServer(("0.0.0.0", PORTA_PAGINA), H)
contexto = ssl.SSLContext(ssl.PROTOCOL_TLS_SERVER)
contexto.load_cert_chain(*certificado_da_pagina())
httpd.socket = contexto.wrap_socket(httpd.socket, server_side=True)
print(f"""
Servidor da medição 3 de pé. Mesma chave no longo e no curto: {cfg['mesma_chave']}.

No iPhone, na mesma rede Wi-Fi do Mac, abra no Safari:

    https://{host}:{PORTA_PAGINA}/

(o aviso de certificado é da página: «Mostrar detalhes → visitar este site»)

Ctrl+C encerra. Resultados em {SAIDA}/iphone.jsonl e servidor-iphone.jsonl.
""", flush=True)
try:
    httpd.serve_forever()
except KeyboardInterrupt:
    pass
finally:
    srv.terminate()
