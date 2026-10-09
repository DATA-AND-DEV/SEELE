#!/usr/bin/env python3
"""A bancada da medição 3, em localhost: Chromium e Safari do Simulador. Descartável.

    cargo build --release
    python3 prova.py chromium      # os casos 1 a 3, e o ocioso medido matando o navegador
    python3 prova.py safari-sim    # os casos 1 a 3 no Safari do Simulador (um simulador ligado)

O binário sobe os endpoints (A, A2, B, MUDO e a porta FECHADA) e registra cada
conexão no stderr. A página `bancada.html` roda os casos de `casos.js` e manda o
resultado por POST. «Ninguém» é o 192.0.2.1 (TEST-NET-1), que não responde.
"""
import http.server, json, os, pathlib, signal, subprocess, sys, tempfile, threading, time


def simulador():
    """O Simulador onde a página abre: `SEELE_SIMULADOR` (o UDID), ou o único ligado.

    Com dois ligados, `booted` é ambíguo, e a medida iria para o aparelho de
    outra frente (a F8 usa o iPhone 17; a F4, o iPhone 18 Pro). Recusar e dizer
    é melhor que medir no aparelho errado.
    """
    escolhido = os.environ.get("SEELE_SIMULADOR")
    if escolhido:
        return escolhido
    ligados = [l.strip() for l in subprocess.run(["xcrun", "simctl", "list", "devices", "booted"],
                                                 capture_output=True, text=True).stdout.splitlines() if "(Booted)" in l]
    if len(ligados) != 1:
        sys.exit(f"há {len(ligados)} Simuladores ligados ({'; '.join(ligados)}): diga qual com SEELE_SIMULADOR=<UDID>")
    return "booted"

AQUI = pathlib.Path(__file__).resolve().parent
BIN = AQUI / "target" / "release" / "alpn-demux"
PORTA = 8091

srv = subprocess.Popen([str(BIN)], stdout=subprocess.PIPE, stderr=subprocess.PIPE, text=True)
cfg = json.loads(srv.stdout.readline())
cfg["host"] = "127.0.0.1"
cfg["ninguem"] = "192.0.2.1"
servidor = []  # (hora local, evento)


def ler_stderr():
    for linha in srv.stderr:
        try:
            servidor.append((time.monotonic(), json.loads(linha)))
        except json.JSONDecodeError:
            servidor.append((time.monotonic(), {"texto": linha.rstrip()}))


threading.Thread(target=ler_stderr, daemon=True).start()

recebido = {}
eventos = []


class H(http.server.BaseHTTPRequestHandler):
    def do_GET(self):
        caminho = self.path.split("?")[0]
        if caminho == "/cfg":
            corpo, tipo = json.dumps(cfg).encode(), "application/json"
        elif caminho == "/casos.js":
            corpo, tipo = (AQUI / "casos.js").read_bytes(), "text/javascript; charset=utf-8"
        else:
            corpo, tipo = (AQUI / "bancada.html").read_bytes(), "text/html; charset=utf-8"
        self.send_response(200)
        self.send_header("content-type", tipo)
        self.send_header("cache-control", "no-store")
        self.end_headers()
        self.wfile.write(corpo)

    def do_POST(self):
        n = int(self.headers.get("content-length", 0))
        corpo = json.loads(self.rfile.read(n))
        if self.path.startswith("/evento"):
            eventos.append((time.monotonic(), corpo))
        else:
            recebido[corpo["nav"]] = corpo
        self.send_response(204)
        self.end_headers()

    def log_message(self, *a):
        pass


httpd = http.server.ThreadingHTTPServer(("127.0.0.1", PORTA), H)
threading.Thread(target=httpd.serve_forever, daemon=True).start()


def esperar(nav, prazo=400):
    fim = time.time() + prazo
    while time.time() < fim and nav not in recebido:
        time.sleep(0.5)
    return recebido.get(nav, {"erro": f"nada voltou em {prazo} s"})


def ocioso_do_chromium():
    """Abre uma sessão no A, mata o navegador sem fechar nada e mede quando o
    servidor desiste. Sem CONNECTION_CLOSE, só o ocioso de 20 s fecha."""
    from playwright.sync_api import sync_playwright
    perfil = tempfile.mkdtemp(prefix="medicao3-ocioso-")
    antes = len(servidor)
    p = sync_playwright().start()
    ctx = p.chromium.launch_persistent_context(perfil)
    ctx.pages[0].goto(f"http://localhost:{PORTA}/?modo=ficar&nav=chromium-ocioso")
    fim = time.time() + 30
    aberta = None
    while time.time() < fim and aberta is None:
        for t, e in servidor[antes:]:
            if e.get("ev") == "aberta" and e.get("ponto") == "A":
                aberta = (t, e)
        time.sleep(0.2)
    pids = [int(x) for x in subprocess.run(["pgrep", "-f", perfil], capture_output=True, text=True).stdout.split()]
    if not aberta or not pids:
        ctx.close(); p.stop()
        return {"erro": f"a sessão não abriu em 30 s (pids {pids})"}
    time.sleep(3)  # alguns pings vão e voltam
    morto = time.monotonic()
    for pid in pids:
        try:
            os.kill(pid, signal.SIGKILL)
        except ProcessLookupError:
            pass
    try:
        p.stop()
    except Exception:
        pass
    fim = time.time() + 60
    while time.time() < fim:
        for t, e in servidor:
            if t > morto and e.get("ev") == "fechou" and e.get("de") == aberta[1]["de"]:
                return {"processos_mortos": len(pids), "servidor_fechou_depois_de_s": round(t - morto, 2),
                        "motivo": e.get("motivo"), "durou_ms": e.get("durou_ms")}
        time.sleep(0.2)
    return {"erro": "o servidor não fechou em 60 s depois de o navegador morrer"}


def resumo(r):
    print(f"\n== {r.get('nav')} · {r.get('ua', '')}")
    if r.get("erro"):
        print("ERRO:", r["erro"])
        return
    for grupo in ("basicos", "lista", "fora"):
        print(f"-- {grupo}")
        for c in r.get(grupo, []):
            estado = "ABRIU" if c.get("abriu") else "recusado/sem resposta"
            extra = f"datagrama={c.get('datagrama_eco')} quadro={c.get('quadro_eco')} max={c.get('maxDatagramSize')}" if c.get("abriu") else c.get("erro", "")
            print(f"   {c['rotulo']:<52} {estado:<22} {c.get('ms')} ms  {extra}")


navs = sys.argv[1:] or ["chromium"]
(AQUI / "resultados").mkdir(exist_ok=True)
for nav in navs:
    url = f"http://localhost:{PORTA}/?modo=bancada&nav={nav}&t={int(time.time())}"
    if nav == "chromium":
        from playwright.sync_api import sync_playwright
        with sync_playwright() as p:
            b = p.chromium.launch()
            pg = b.new_page()
            pg.goto(url)
            r = esperar(nav)
            r["navegador"] = b.version
            b.close()
        r["ocioso"] = ocioso_do_chromium()
    elif nav == "safari-sim":
        subprocess.run(["xcrun", "simctl", "openurl", simulador(), url], check=True)
        r = esperar(nav)
    else:
        raise SystemExit(f"navegador desconhecido: {nav}")
    r["cfg"] = cfg
    r["servidor"] = [e for _, e in servidor]
    (AQUI / "resultados" / f"{nav}.json").write_text(json.dumps(r, ensure_ascii=False, indent=1))
    resumo(r)
    if "ocioso" in r:
        print("-- ocioso (navegador morto sem fechar):", r["ocioso"])

srv.terminate()
httpd.shutdown()
print("\n-- o que o servidor viu fechar ou falhar")
for _, e in servidor:
    if e.get("ev") in ("falhou", "fechou", "mudo_primeiro_pacote", "cliente_seele1"):
        print("  ", json.dumps(e, ensure_ascii=False))
