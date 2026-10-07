#!/usr/bin/env python3
"""Safari do Simulador contra o mesmo quinn::Endpoint com dois ALPNs. Descartável."""
import http.server, json, os, subprocess, threading, time, pathlib

AQUI = pathlib.Path(__file__).resolve().parent
srv = subprocess.Popen([str(AQUI / "target/release/alpn-demux")], stdout=subprocess.PIPE, stderr=subprocess.PIPE, text=True)
cfg = json.loads(srv.stdout.readline())
print("servidor:", cfg)

def adulterar(h):
    b = bytearray.fromhex(h); b[0] ^= 1; return b.hex()

casos = [
    ["A h3, hash do curto", cfg["porta_a"], cfg["curto"]],
    ["A h3, hash do curto com um bit trocado", cfg["porta_a"], adulterar(cfg["curto"])],
    ["A h3, 60 hashes, o certo por último", cfg["porta_a"], ",".join([os.urandom(32).hex() for _ in range(59)] + [cfg["curto"]])],
    ["B h3, só o longo (1975-4096), hash certo", cfg["porta_b"], cfg["longo"]],
]

PAGINA = """<!doctype html><meta charset=utf-8><title>demux</title><pre id=o>rodando</pre><script>
const CASOS = %s;
async function um([rotulo, porta, hash]) {
  const r = {rotulo};
  const hs = hash.split(',').map(h => ({algorithm: 'sha-256', value: new Uint8Array(h.match(/../g).map(x => parseInt(x, 16)))}));
  r.quantos_hashes = hs.length;
  const t0 = performance.now();
  let wt;
  try {
    wt = new WebTransport(`https://127.0.0.1:${porta}/seele/1`, {serverCertificateHashes: hs});
    await Promise.race([wt.ready, new Promise((_, n) => setTimeout(() => n(new Error('prazo de 8 s')), 8000))]);
    r.abriu_ms = Math.round(performance.now() - t0);
  } catch (e) { r.erro = String(e); r.ms = Math.round(performance.now() - t0); try { wt && wt.close(); } catch (_) {} return r; }
  const w = wt.datagrams.createWritable ? wt.datagrams.createWritable().getWriter() : wt.datagrams.writable.getWriter();
  const rd = wt.datagrams.readable.getReader();
  await w.write(new Uint8Array([8, 1, 2, 3, 4]));
  const d = await Promise.race([rd.read(), new Promise(res => setTimeout(() => res(null), 3000))]);
  r.datagrama_eco = d && d.value ? Array.from(d.value) : null;
  r.maxDatagramSize = wt.datagrams.maxDatagramSize;
  const s = await wt.createBidirectionalStream();
  const sw = s.writable.getWriter();
  const corpo = new TextEncoder().encode('\\x08ola');
  const q = new Uint8Array(4 + corpo.length);
  new DataView(q.buffer).setUint32(0, corpo.length); q.set(corpo, 4);
  await sw.write(q); await sw.close();
  const sr = s.readable.getReader(); let tudo = [];
  while (true) { const {value, done} = await sr.read(); if (done) break; tudo.push(...value); }
  r.quadro_eco_ok = JSON.stringify(tudo) === JSON.stringify(Array.from(q));
  wt.close();
  return r;
}
(async () => {
  const res = [];
  for (const c of CASOS) res.push(await um(c));
  document.getElementById('o').textContent = JSON.stringify(res, null, 1);
  await fetch('/r', {method: 'POST', body: JSON.stringify({ua: navigator.userAgent, res})});
})();
</script>""" % json.dumps(casos)

recebido = {}
class H(http.server.BaseHTTPRequestHandler):
    def do_GET(self):
        self.send_response(200); self.send_header("content-type", "text/html; charset=utf-8"); self.end_headers()
        self.wfile.write(PAGINA.encode())
    def do_POST(self):
        n = int(self.headers.get("content-length", 0))
        recebido["r"] = json.loads(self.rfile.read(n))
        self.send_response(204); self.end_headers()
    def log_message(self, *a): pass

httpd = http.server.HTTPServer(("127.0.0.1", 8091), H)
threading.Thread(target=httpd.serve_forever, daemon=True).start()
subprocess.run(["xcrun", "simctl", "openurl", "booted", f"http://localhost:8091/?t={int(time.time())}"], check=True)
for _ in range(90):
    if "r" in recebido: break
    time.sleep(0.5)
print(json.dumps(recebido.get("r", {"erro": "nada voltou em 45 s"}), ensure_ascii=False, indent=1))
srv.terminate()
print("--- stderr do servidor ---")
print(srv.stderr.read())
