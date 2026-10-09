#!/usr/bin/env python3
"""Medição 1 do ADR 0057: a assinatura Ed25519 do navegador passa no verify do seeled?

Serve `pagina.html` em http://localhost:8093, abre a página em cada navegador
pedido, colhe as assinaturas e as passa pelo verificador
(`target/release/spike-ed25519-do-navegador`), que tem as linhas de
`session.rs:550-599`. Descartável.

    cargo build --release
    python3 prova.py chromium safari-mac safari-sim
    python3 prova.py iphone      # HTTPS na LAN; o dono abre o endereço impresso

No modo `iphone`, a página vai por HTTPS com um certificado autoassinado, porque
o WebCrypto só existe em contexto seguro e o iPhone não alcança o `localhost`
do Mac. O Safari avisa do certificado uma vez («Mostrar detalhes → visitar
este site»), como no `voz-no-navegador`.
"""
import http.server, json, pathlib, ssl, subprocess, sys, threading, time
import os


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
VERIFICADOR = AQUI / "target" / "release" / "spike-ed25519-do-navegador"
PORTA = 8093
PAGINA = (AQUI / "pagina.html").read_bytes()

colhido = {}
trava = threading.Lock()


class H(http.server.BaseHTTPRequestHandler):
    def do_GET(self):
        self.send_response(200)
        self.send_header("content-type", "text/html; charset=utf-8")
        self.send_header("cache-control", "no-store")
        self.end_headers()
        self.wfile.write(PAGINA)

    def do_POST(self):
        n = int(self.headers.get("content-length", 0))
        r = json.loads(self.rfile.read(n))
        with trava:
            colhido[(r["nav"], r["fase"])] = r
        self.send_response(204)
        self.end_headers()

    def log_message(self, *a):
        pass


def verificar(linhas):
    """Cada linha é (chave, mensagem, assinatura) em hex; devolve os vereditos."""
    entrada = "".join(f"{k} {m or '-'} {s}\n" for k, m, s in linhas)
    saida = subprocess.run([str(VERIFICADOR)], input=entrada, capture_output=True, text=True, check=True).stdout
    return [json.loads(l) for l in saida.splitlines()]


def virar_bit(h, byte, mascara=1):
    b = bytearray.fromhex(h)
    b[byte] ^= mascara
    return b.hex()


def ip_da_lan():
    for interface in ("en0", "en1"):
        r = subprocess.run(["ipconfig", "getifaddr", interface], capture_output=True, text=True)
        if r.stdout.strip():
            return r.stdout.strip()
    raise SystemExit("sem endereço na LAN (en0/en1)")


def certificado_da_pagina():
    pasta = AQUI / "resultados"
    pasta.mkdir(exist_ok=True)
    cert, chave = pasta / "pagina.pem", pasta / "pagina.key"
    if not cert.exists():
        subprocess.run(["openssl", "req", "-x509", "-newkey", "ec", "-pkeyopt", "ec_paramgen_curve:prime256v1", "-pkeyopt", "ec_param_enc:named_curve",
                        "-nodes", "-days", "7", "-subj", "/CN=seele-medicao-1",
                        "-keyout", str(chave), "-out", str(cert)], check=True, capture_output=True)
    return cert, chave


def abrir(nav):
    url = f"http://localhost:{PORTA}/?nav={nav}&t={int(time.time())}"
    if nav == "iphone":
        print(f"\nNo iPhone, na mesma rede, abra:  https://{ip_da_lan()}:{PORTA}/?nav=iphone\n", flush=True)
        return None
    if nav == "safari-mac":
        subprocess.run(["open", "-a", "Safari", url], check=True)
        return None
    if nav == "safari-sim":
        subprocess.run(["xcrun", "simctl", "openurl", simulador(), url], check=True)
        return None
    if nav == "chromium":
        from playwright.sync_api import sync_playwright
        p = sync_playwright().start()
        b = p.chromium.launch()
        pg = b.new_page()
        pg.goto(url)
        return (p, b)
    raise SystemExit(f"navegador desconhecido: {nav}")


def esperar(nav, prazo=60):
    if nav == "iphone":
        prazo = 600
    fim = time.time() + prazo
    while time.time() < fim:
        with trava:
            f1, f2 = colhido.get((nav, 1)), colhido.get((nav, 2))
        if (f1 and f2) or (f1 and f1.get("erro")) or (f2 and f2.get("erro")):
            return f1, f2
        time.sleep(0.25)
    return colhido.get((nav, 1)), colhido.get((nav, 2))


def medir(nav):
    aberto = abrir(nav)
    f1, f2 = esperar(nav)
    if aberto:
        p, b = aberto
        b.close()
        p.stop()
    res = {"nav": nav}
    if not f1 or f1.get("erro"):
        res["erro"] = (f1 or {}).get("erro", "a fase 1 não voltou no prazo")
        return res
    pub = f1["publica"]
    entradas = f1["entradas"] + ((f2 or {}).get("entradas") or [])
    chaves = [pub] * len(f1["entradas"]) + [(f2 or {}).get("publica", pub)] * len((f2 or {}).get("entradas") or [])
    vered = verificar([(k, e["m"], e["s"]) for k, e in zip(chaves, entradas)])

    por_familia = {}
    for e, v in zip(entradas, vered):
        f = por_familia.setdefault(e["familia"], {"total": 0, "verify": 0, "estrito": 0, "propria": 0, "falhas": []})
        f["total"] += 1
        f["verify"] += v["verify"]
        f["estrito"] += v["estrito"]
        f["propria"] += bool(e["propria"])
        if e["familia"] == "borda":
            f.setdefault("por_tamanho", []).append({"bytes": len(e["m"]) // 2, "verify": v["verify"], "propria": e["propria"]})
        if not v["verify"]:
            f["falhas"].append({"chave": pub, "m": e["m"], "s": e["s"], "erro": v["erro"]})

    # A prova contra a regressão: um bit trocado na assinatura, na mensagem, ou
    # um `s` fora do intervalo tem de ser recusado. Sem isto, um verificador que
    # aceitasse tudo daria o mesmo «passa».
    amostra = [e for e in f1["entradas"] if e["familia"] in ("nonce", "s2")][:20]
    adulteradas = []
    for e in amostra:
        adulteradas.append((pub, e["m"], virar_bit(e["s"], 0)))  # R
        adulteradas.append((pub, e["m"], virar_bit(e["s"], 40)))  # s
        adulteradas.append((pub, virar_bit(e["m"], 0), e["s"]))  # mensagem
        adulteradas.append((pub, e["m"], virar_bit(e["s"], 63, 0x80)))  # s não canônico
    vadult = verificar(adulteradas)

    res.update({
        "ua": f1["ua"],
        "seguro": f1["seguro"],
        "privada_extraivel": f1["privada_extraivel"],
        "exportou_a_privada": f1["exportou_a_privada"],
        "publica_bytes": len(pub) // 2,
        "assinatura_bytes": len(f1["entradas"][0]["s"]) // 2,
        "gerar_ms": round(f1["gerar_ms"], 2),
        "assinar_ms_p50": round(f1["assinar_ms_p50"], 3),
        "assinar_ms_p95": round(f1["assinar_ms_p95"], 3),
        "distintas_para_a_mesma_mensagem": f1["assinaturas_distintas_da_mesma_mensagem"],
        "familias": por_familia,
        "fase2_leu_a_chave": (f2 or {}).get("leu_a_chave"),
        "fase2_mesma_publica": (f2 or {}).get("publica") == pub,
        "fase2_privada_extraivel": (f2 or {}).get("privada_extraivel"),
        "adulteradas": {"total": len(vadult), "aceitas": sum(v["verify"] for v in vadult)},
    })
    return res


def main():
    navs = sys.argv[1:] or ["chromium"]
    if not VERIFICADOR.exists():
        raise SystemExit("rode `cargo build --release` antes")
    if "iphone" in navs and len(navs) > 1:
        raise SystemExit("o modo iphone roda sozinho")
    if navs == ["iphone"]:
        httpd = http.server.ThreadingHTTPServer(("0.0.0.0", PORTA), H)
        contexto = ssl.SSLContext(ssl.PROTOCOL_TLS_SERVER)
        contexto.load_cert_chain(*certificado_da_pagina())
        httpd.socket = contexto.wrap_socket(httpd.socket, server_side=True)
    else:
        httpd = http.server.ThreadingHTTPServer(("127.0.0.1", PORTA), H)
    threading.Thread(target=httpd.serve_forever, daemon=True).start()
    (AQUI / "resultados").mkdir(exist_ok=True)
    for nav in navs:
        r = medir(nav)
        (AQUI / "resultados" / f"{nav}.json").write_text(json.dumps(r, ensure_ascii=False, indent=1))
        resumo = {k: v for k, v in r.items() if k != "familias"}
        resumo["familias"] = {f: {k: v for k, v in d.items() if k != "falhas"} | {"falhas": len(d["falhas"])} for f, d in (r.get("familias") or {}).items()}
        print(json.dumps(resumo, ensure_ascii=False, indent=1))
    httpd.shutdown()


if __name__ == "__main__":
    main()
