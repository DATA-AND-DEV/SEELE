#!/usr/bin/env python3
"""Chromium contra um quinn::Endpoint com dois ALPNs. Descartável."""
import http.server, json, subprocess, sys, threading, pathlib
from playwright.sync_api import sync_playwright

AQUI = pathlib.Path(__file__).resolve().parent
bin_ = AQUI / "target" / "release" / "alpn-demux"
srv = subprocess.Popen([str(bin_)], stdout=subprocess.PIPE, stderr=subprocess.PIPE, text=True)
linha = srv.stdout.readline()
cfg = json.loads(linha)
print("servidor:", cfg)

class H(http.server.SimpleHTTPRequestHandler):
    def do_GET(self):
        self.send_response(200); self.send_header("content-type", "text/html"); self.end_headers()
        self.wfile.write(b"<!doctype html><title>t</title>")
    def log_message(self, *a): pass

httpd = http.server.HTTPServer(("127.0.0.1", 0), H)
threading.Thread(target=httpd.serve_forever, daemon=True).start()

JS = """
async ({porta, hash, rotulo}) => {
  const r = {rotulo};
  const hs = hash.split(',').map(h => ({algorithm: 'sha-256', value: new Uint8Array(h.match(/../g).map(x => parseInt(x, 16)))}));
  r.quantos_hashes = hs.length;
  const t0 = performance.now();
  let wt;
  try {
    wt = new WebTransport(`https://127.0.0.1:${porta}/seele/1`, {
      serverCertificateHashes: hs,
    });
    await wt.ready;
    r.abriu_ms = Math.round(performance.now() - t0);
  } catch (e) { r.erro = String(e); r.ms = Math.round(performance.now() - t0); return r; }
  // datagrama de eco
  const w = wt.datagrams.writable.getWriter();
  const rd = wt.datagrams.readable.getReader();
  await w.write(new Uint8Array([8, 1, 2, 3, 4]));
  const d = await Promise.race([rd.read(), new Promise(res => setTimeout(() => res(null), 3000))]);
  r.datagrama_eco = d && d.value ? Array.from(d.value) : null;
  r.maxDatagramSize = wt.datagrams.maxDatagramSize;
  // quadro de 4 bytes BE + corpo num fluxo bidirecional
  const s = await wt.createBidirectionalStream();
  const sw = s.writable.getWriter();
  const corpo = new TextEncoder().encode('\\x08ola');
  const q = new Uint8Array(4 + corpo.length);
  new DataView(q.buffer).setUint32(0, corpo.length);
  q.set(corpo, 4);
  await sw.write(q); await sw.close();
  const sr = s.readable.getReader(); let tudo = [];
  while (true) { const {value, done} = await sr.read(); if (done) break; tudo.push(...value); }
  r.quadro_eco_ok = JSON.stringify(tudo) === JSON.stringify(Array.from(q));
  wt.close();
  return r;
}
"""

def adulterar(h):
    b = bytearray.fromhex(h); b[0] ^= 1; return b.hex()

casos = [
    ("A h3, hash do curto", cfg["porta_a"], cfg["curto"]),
    ("A h3, hash do curto com um bit trocado", cfg["porta_a"], adulterar(cfg["curto"])),
    ("A h3, hash do longo (o servidor apresenta o curto)", cfg["porta_a"], cfg["longo"]),
    ("A h3, 60 hashes, o certo por último", cfg["porta_a"], ",".join([__import__('os').urandom(32).hex() for _ in range(59)] + [cfg["curto"]])),
    ("B h3, só o longo (1975-4096), hash certo", cfg["porta_b"], cfg["longo"]),
]
with sync_playwright() as p:
    nav = p.chromium.launch()
    pag = nav.new_page()
    pag.goto(f"http://localhost:{httpd.server_address[1]}/")
    print("navegador:", nav.version)
    for rotulo, porta, h in casos:
        print(json.dumps(pag.evaluate(JS, {"porta": porta, "hash": h, "rotulo": rotulo}), ensure_ascii=False))
    nav.close()
srv.terminate()
print("--- stderr do servidor ---")
print(srv.stderr.read())
