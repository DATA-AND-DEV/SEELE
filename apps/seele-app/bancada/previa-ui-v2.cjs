// **A prévia da UI v2** (0.17): a entrada em largura total e a ligação pessoal,
// servidas localmente com DADOS DE EXEMPLO, para abrir num navegador e ver.
//
// Não é um teste: serve a página e, com `--capturas=<pasta>`, tira capturas.
// As provas funcionais moram em `vista-pessoal-0-17.cjs` e no `frontend.rs`.
//
// # O que é simulado e o que é o produto
//
// SIMULADO: a ponte (um `window.__TAURI__` com respostas combinadas na entrada;
// duas sessões de mentira na `ponte-web.js` real na ligação) e todos os dados
// (nomes e códigos de exemplo, óbvios: «Luna Exemplo», «Servidor Fictício
// Aurora»). REAL: o `index.html`, o CSS e o JS de `ui/`.
//
// Subir:   node apps/seele-app/bancada/previa-ui-v2.cjs [--porta=8731]
// Cenas:   /?cena=entrada | entrada-vazia | ligacao | ligacao-fora | servidor
// Capturas: node apps/seele-app/bancada/previa-ui-v2.cjs --capturas=/tmp/ui-v2
//           (Chromium e WebKit, em 1000, 1280, 1920 e um tamanho de celular)

const http = require("node:http");
const fs = require("node:fs");
const path = require("node:path");
const { respostas, retrato, CSP_DO_DESKTOP } = require("./telas.cjs");

const raiz = path.resolve(__dirname, "../ui");
const TIPOS = { ".html": "text/html; charset=utf-8", ".js": "text/javascript; charset=utf-8", ".css": "text/css; charset=utf-8", ".woff2": "font/woff2", ".png": "image/png", ".svg": "image/svg+xml" };
const arg = (nome) => process.argv.find((a) => a.startsWith(`--${nome}=`))?.slice(nome.length + 3);

// ------------------------------------------------------------ dados de exemplo

const agora = Math.floor(Date.now() / 1000);
const AMIGOS = [
  { codigo: "LUNA-EXMP-0001-AAAA", nome: "Luna Exemplo", apelido: "luna", visto_em: agora - 3600, direcao: "recebi", bloqueado: false, amigo: true, alvo: "@LUNA-EXMP-0001-AAAA" },
  { codigo: "DAVI-TEST-0002-BBBB", nome: "Davi Teste de Sobrenome Muito Longo da Silva", apelido: null, visto_em: null, direcao: "liguei", bloqueado: false, amigo: true, alvo: "@DAVI-TEST-0002-BBBB" },
  { codigo: "BETO-FICT-0003-CCCC", nome: "Beto Fictício", apelido: null, visto_em: null, direcao: null, bloqueado: true, amigo: true, alvo: "@BETO-FICT-0003-CCCC" },
];
const SERVIDORES = [
  { alvo: "aurora.exemplo:8383", apelido: "Alex", nome: "Servidor Fictício Aurora", visto_em: agora - 7200, icone: null },
  { alvo: "oficina.exemplo:8383", apelido: "Alex", nome: "Oficina de Mods (exemplo)", visto_em: agora - 86400 * 3, icone: null },
  { alvo: "192.0.2.10:8383", apelido: null, nome: null, visto_em: agora - 86400 * 20, icone: null },
];

function tabelaDaEntrada(vazia) {
  return respostas({
    meu_codigo: "ALEX-EXMP-9999-ZZZZ",
    estado_da_sala: { estado: "NoAr", registro: { registro: "Registrado" }, firewall: null },
    amigos: vazia ? [] : AMIGOS,
    contatos_da_agenda: AMIGOS,
    conhecidos: vazia ? [] : SERVIDORES,
    sessoes: null,
    meu_servidor: null,
  });
}

// --------------------------------------------------------------- o que se injeta

function scriptDoTauri(tabela) {
  return `(() => {
  const t = ${JSON.stringify(tabela)};
  window.__TAURI__ = {
    core: { invoke: async (cmd) => (Object.hasOwn(t, cmd) ? t[cmd] : null) },
    event: { listen: async () => () => {} },
    window: { getCurrentWindow: () => ({ onCloseRequested: () => {}, isMaximized: async () => false }) },
  };
})();`;
}

function scriptDaLigacao(dentro, vistaServidor) {
  return `(async () => {
  const base = ${JSON.stringify(retrato())};
  const pessoa = (id, nickname) => ({ id, nickname, speaking: false, muted: false, total_isolation: false, signal: 80, sync_band: "Nominal", is_self: false });
  const falsa = (nome, extra, outros, sentado) => {
    const sala = { ...base, server: nome, voice_rooms: [{ id: 1, name: extra.sala, limit: 8, password_required: false, occupied_by_us: sentado, channel: 1,
      people: [{ id: 1, nickname: "Alex", speaking: false, muted: false, total_isolation: false, signal: 92, sync_band: "Nominal", is_self: true }, ...outros], sync: null }] };
    const s = {
      alvo: { endereco: extra.endereco, impressao: nome }, acabou: false, ligacao: extra.ligacao ?? null,
      retrato: () => ({ sala, contados: {}, telemetria: { rtt_ms: 10, jitter_ms: 1, loss_fraction: 0, signal: 90, sync_band: "Nominal" } }),
      enlace: () => "Online",
      atender: (comando) => comando === "sala_da_voz" ? { resposta: { sala: null, voz: 0 } } : comando === "messages" ? { resposta: [] } : { resposta: null },
      encerrar: async () => { s.acabou = true; },
    };
    s.snapshot = () => retratoDaSessao(s);
    return s;
  };
  await new Promise((r) => (document.readyState === "complete" ? r() : window.addEventListener("load", r)));
  await new Promise((r) => setTimeout(r, 300));
  sessaoDaPonte.atual = falsa("Servidor Fictício Aurora", { sala: "Sala principal", endereco: "aurora.exemplo:8383" }, [pessoa(2, "Davi Teste")], true);
  sessaoDaPonte.atual = falsa("Luna Exemplo", { sala: "Ligação", endereco: "@LUNA-EXMP-0001-AAAA", ligacao: "LUNA-EXMP-0001-AAAA" }, [pessoa(2, "Luna Exemplo")], ${dentro ? "true" : "false"});
  const entrada = await invoke("ver_sessao", { sessao: ${vistaServidor ? 1 : 2} });
  for (const t of document.querySelectorAll("section.tela")) t.hidden = true;
  $("tela-sessao").hidden = false;
  desenhar(entrada.snapshot);
  await recarregarSessoes();
})();`;
}

const FAIXA = `(() => {
  const f = document.createElement("div");
  f.textContent = "PRÉVIA · dados de exemplo";
  f.style.cssText = "position:fixed;right:150px;top:2px;z-index:9999;padding:2px 8px;font:11px monospace;letter-spacing:.1em;color:#000;background:#9a9a9a;pointer-events:none;opacity:.85";
  document.addEventListener("DOMContentLoaded", () => document.body.append(f));
})();`;

function paginaDaCena(cena) {
  let html = fs.readFileSync(path.join(raiz, "index.html"), "utf8");
  const cabeca = ['<script src="/__previa/faixa.js"></script>'];
  const pe = [];
  if (cena === "entrada" || cena === "entrada-vazia") {
    cabeca.unshift(`<script src="/__previa/tauri-${cena}.js"></script>`);
  } else {
    cabeca.unshift('<script src="/__previa/larga.js"></script>');
    pe.push(`<script src="/__previa/${cena}.js"></script>`);
  }
  // No COMEÇO do `<head>`: o `__TAURI__` simulado (e a casca larga) têm de
  // existir antes de a `ponte-web.js` decidir se o navegador ocupa o lugar do app.
  html = html.replace(/<head[^>]*>/, (abre) => `${abre}${cabeca.join("")}`).replace("</body>", `${pe.join("")}</body>`);
  return html;
}

const VIRTUAIS = {
  "/__previa/faixa.js": () => FAIXA,
  "/__previa/larga.js": () => 'try { localStorage.setItem("seele.casca", "larga"); } catch {}',
  "/__previa/tauri-entrada.js": () => scriptDoTauri(tabelaDaEntrada(false)),
  "/__previa/tauri-entrada-vazia.js": () => scriptDoTauri(tabelaDaEntrada(true)),
  "/__previa/ligacao.js": () => scriptDaLigacao(true, false),
  "/__previa/ligacao-fora.js": () => scriptDaLigacao(false, false),
  "/__previa/servidor.js": () => scriptDaLigacao(true, true),
};

function servidor() {
  return http.createServer((req, res) => {
    const url = new URL(req.url, "http://x");
    const pedido = decodeURIComponent(url.pathname);
    res.setHeader("Content-Security-Policy", CSP_DO_DESKTOP);
    if (VIRTUAIS[pedido]) {
      res.setHeader("Content-Type", TIPOS[".js"]);
      return res.end(VIRTUAIS[pedido]());
    }
    if (pedido === "/") {
      const cena = url.searchParams.get("cena") ?? "entrada";
      res.setHeader("Content-Type", TIPOS[".html"]);
      return res.end(paginaDaCena(cena));
    }
    const alvo = path.join(raiz, pedido);
    if (!alvo.startsWith(raiz) || !fs.existsSync(alvo) || fs.statSync(alvo).isDirectory()) {
      res.statusCode = 404;
      return res.end("nao encontrado");
    }
    res.setHeader("Content-Type", TIPOS[path.extname(alvo)] ?? "application/octet-stream");
    res.end(fs.readFileSync(alvo));
  });
}

// ------------------------------------------------------------------ as capturas

async function capturar(porta, pasta) {
  const pw = require("./playwright.cjs");
  fs.mkdirSync(pasta, { recursive: true });
  const janelas = [["1000x760", 1000, 760], ["1280x800", 1280, 800], ["1920x1080", 1920, 1080], ["celular-390x844", 390, 844]];
  const cenas = ["entrada", "entrada-vazia", "ligacao", "ligacao-fora", "servidor"];
  for (const [nomeDoMotor, motor] of [["chromium", pw.chromium()], ["webkit", pw.webkit()]]) {
    const navegador = await motor.launch({ headless: true });
    try {
      for (const [rotulo, largura, altura] of janelas) {
        for (const cena of cenas) {
          const pagina = await navegador.newPage({ viewport: { width: largura, height: altura } });
          const erros = [];
          pagina.on("pageerror", (e) => erros.push(e.message));
          await pagina.goto(`http://127.0.0.1:${porta}/?cena=${cena}`);
          await pagina.waitForTimeout(900);
          const rolagemX = await pagina.evaluate(() => document.documentElement.scrollWidth > innerWidth + 1);
          await pagina.screenshot({ path: path.join(pasta, `${cena}-${nomeDoMotor}-${rotulo}.png`) });
          console.log(`${cena} ${nomeDoMotor} ${rotulo}${rolagemX ? " ROLAGEM-LATERAL" : ""}${erros.length ? " ERROS " + erros.join("|") : ""}`);
          await pagina.close();
        }
      }
    } finally {
      await navegador.close();
    }
  }
}

(async () => {
  const srv = servidor();
  const pedida = Number(arg("porta") ?? (arg("capturas") ? 0 : 8731));
  await new Promise((ok) => srv.listen(pedida, "127.0.0.1", ok));
  const porta = srv.address().port;
  const pasta = arg("capturas");
  if (pasta) {
    await capturar(porta, pasta);
    srv.close();
    return;
  }
  console.log(`prévia da UI v2 (dados de exemplo): http://127.0.0.1:${porta}/?cena=entrada`);
  for (const c of ["entrada-vazia", "ligacao", "ligacao-fora", "servidor"]) console.log(`  http://127.0.0.1:${porta}/?cena=${c}`);
})();
