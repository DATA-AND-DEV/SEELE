// As telas do produto num navegador, com a ponte do Tauri simulada.
//
// # Por que ela existe
//
// A validação visual de 20/09/2026 terminou com cinco linhas da matriz em
// branco — moderação, anexos, fim/reconexão, admissão e janela pequena —, e
// todas pelo mesmo motivo: elas exigem **ver** a tela, e ver a tela exigia o
// aplicativo nativo e uma pessoa na frente dele.
//
// Este arquivo tira a segunda exigência. Ele serve `ui/` como está, injeta um
// `window.__TAURI__` com respostas combinadas, e deixa o resto do produto
// rodar: os mesmos scripts, as mesmas folhas, o mesmo `index.html`. O que se
// vê é o desenho de verdade.
//
// # O que ele **não** é
//
// Não é o aplicativo. Não há Rust do outro lado, não há áudio, não há rede, e
// `invoke` devolve o que esta bancada combinou em vez do que o produto
// calcularia. Ele responde «esta tela, com este estado, cabe e é alcançável?»
// — e não «o produto faz o que promete».
//
// Um retrato daqui não homologa jornada nenhuma. Ele encontra corte, controle
// fora de alcance e rolagem perdida, que é exatamente o que a matriz listou
// como pendente e o que uma suíte de texto não vê.
//
//   node apps/seele-app/bancada/telas.cjs <cena> <saida.png> [largura] [altura]
//
// As cenas estão em `CENAS`, no fim do arquivo.

const http = require("node:http");
const fs = require("node:fs");
const path = require("node:path");

const raiz = path.resolve(__dirname, "../ui");
const TIPOS = {
  ".html": "text/html; charset=utf-8",
  ".js": "text/javascript; charset=utf-8",
  ".css": "text/css; charset=utf-8",
  ".woff2": "font/woff2",
  ".png": "image/png",
  ".svg": "image/svg+xml",
};

/** O servidor de arquivos: `ui/` como está, sem transformação nenhuma. */
function servir() {
  return http.createServer((req, res) => {
    const pedido = decodeURIComponent(req.url.split("?")[0]);
    const alvo = path.join(raiz, pedido === "/" ? "index.html" : pedido);
    if (!alvo.startsWith(raiz) || !fs.existsSync(alvo) || fs.statSync(alvo).isDirectory()) {
      res.statusCode = 404;
      res.end("nao encontrado");
      return;
    }
    // **A mesma política do produto.** `tauri.conf.json` declara
    // `style-src 'self'`, e foi ela que descartou por meses as folhas que um
    // MOD declarava. Uma bancada mais permissiva que o produto aprovaria o
    // que o produto recusa.
    res.setHeader(
      "Content-Security-Policy",
      "default-src 'self'; style-src 'self'; script-src 'self'; img-src 'self' data:; font-src 'self'",
    );
    res.setHeader("Content-Type", TIPOS[path.extname(alvo)] ?? "application/octet-stream");
    res.end(fs.readFileSync(alvo));
  });
}

// ------------------------------------------------------------- o estado base

/** Uma pessoa do retrato, com o sinal que dá caráter à lista. */
const pessoa = (id, nickname, extra = {}) => ({
  id,
  nickname,
  speaking: false,
  muted: false,
  total_isolation: false,
  signal: 92,
  sync_band: "Nominal",
  is_self: false,
  ...extra,
});

/** O retrato que a sessão desenha. Os nomes são os que o Rust manda. */
function retrato(extra = {}) {
  return {
    caminho: "RedeLocal",
    link: "Online",
    link_state: "Verified",
    server: "Casa do Lago",
    icon_revision: 0,
    person_icons_revision: 0,
    me: 1,
    nickname: "Alex",
    voice_rooms: [
      {
        id: 1,
        name: "Sala principal",
        limit: 8,
        password_required: false,
        occupied_by_us: true,
        channel: 1,
        people: [pessoa(1, "Alex", { is_self: true }), pessoa(2, "Lia", { signal: 71, sync_band: "Degradado" })],
        sync: null,
      },
    ],
    presentes: [
      pessoa(1, "Alex", { is_self: true }),
      pessoa(2, "Lia", { signal: 71, sync_band: "Degradado" }),
      pessoa(3, "Rafa", { signal: 44, sync_band: "Critico", muted: true }),
    ],
    channels: [
      { id: 1, name: "geral", open: true },
      { id: 2, name: "combinados", open: false },
    ],
    messages_revision: 1,
    telemetry: {
      rtt_ms: 18.4,
      jitter_ms: 3.1,
      loss_fraction: 0.004,
      bitrate_bps: 64000,
      signal: 92,
      sync_band: "Nominal",
      input_level: 0.2,
      local_fault: false,
      frames_refused: 0,
    },
    notice: null,
    muted: false,
    total_isolation: false,
    speaking: false,
    voice_mode: "PushToTalk",
    audio_available: true,
    capture: { id: "mic", name: "Microfone interno", is_default: true },
    playback: { id: "saida", name: "Saída padrão", is_default: true },
    aparelho: "Pronto",
    trocas_de_aparelho: 0,
    may_manage_voice_rooms: true,
    may_kick: true,
    may_ban: true,
    may_remove_message: true,
    may_move_person: true,
    may_customise_server: true,
    may_delete_rooms: true,
    tela: null,
    transmissoes: [],
    ended: null,
    ...extra,
  };
}

/** Os nomes são os do `Message` do Rust, e não uma invenção desta bancada. */
const agora = Math.floor(Date.now() / 1000);
const MENSAGENS = [
  {
    id: 1, channel: 1, author: 2, author_nickname: "Lia", at_seconds: agora - 600,
    body: "Cheguei. O túnel do norte está fechado até amanhã — o desvio antigo continua aberto, mas some no escuro.",
    own: false, edited: false, attachment: null,
  },
  {
    id: 2, channel: 1, author: 1, author_nickname: "Alex", at_seconds: agora - 480,
    body: "Combinado. Levo o desvio antigo.", own: true, edited: false, attachment: null,
  },
];

/** O que cada comando devolve. Uma cena substitui o que precisar. */
function respostas(extra = {}) {
  return {
    abertura: null,
    plataforma: "macos",
    apelido_local: "Alex",
    meu_retrato: null,
    versoes_instaladas: [],
    servidores_guardados: [],
    conhecidos: [],
    mods_instalados: [],
    aceite_de_mods: { aceitos: [] },
    messages: MENSAGENS,
    estado_da_porta: { aberta: true, senha: false, pedidos: [] },
    // A forma é a de `PreviewRules` no Rust; uma inventada faz a conversa
    // inteira sumir com um `TypeError` dentro do desenho do anexo.
    regras_de_previa: { limit: 8 * 1024 * 1024, types: ["image/png", "image/jpeg", "image/webp"] },
    pasta_de_downloads: "/tmp",
    telemetria_da_malha: null,
    ...extra,
  };
}

// ------------------------------------------------------------------ as cenas

const CENAS = {
  /** A sessão desenhada, que é a base de quase tudo. */
  sessao: { resposta: respostas(), retrato: retrato(), depois: "sincronizarMensagens(99)" },

  /** A gaveta de pessoas, que só existe quando a faixa recolhe. */
  "pessoas-gaveta": {
    resposta: respostas(),
    retrato: retrato(),
    depois: "sincronizarMensagens(99); alternarPessoas(true);",
  },

  /** A gaveta de canais, pelo mesmo motivo. */
  "canais-gaveta": {
    resposta: respostas(),
    retrato: retrato(),
    depois: "sincronizarMensagens(99); alternarCanais(true);",
  },

  /** A moderação de uma pessoa: expulsar, banir, mover. */
  moderacao: { resposta: respostas(), retrato: retrato(), depois: 'abrirModeracao("2")' },

  /** A confirmação que a moderação faz antes de um ato irreversível. */
  "moderacao-confirmar": {
    resposta: respostas(),
    retrato: retrato(),
    depois: 'abrirModeracao("2"); document.querySelector(\'[data-ato="banir"]\')?.click();',
  },

  /** A portaria com um pedido de admissão esperando. */
  portaria: {
    resposta: respostas({
      // Os nomes são os de `EstadoDaPorta` e `PedidoDaPortaria` no Rust.
      estado_da_porta: {
        hospedando: true,
        aberto: true,
        tem_senha: true,
        aceita_convites: true,
        portaria_ligada: true,
        pendentes: 1,
        alcance: "RedeLocal",
      },
      pedidos_da_portaria: [{
        impressao: "9f2c8a1140de77b31c05e6f92a84db30",
        apelido: "quem-bate",
        segredo: "senha",
        observacao: "Sou a Iria, do turno da tarde.",
        bateu_em: Math.floor(Date.now() / 1000) - 45,
        batidas: 2,
      }],
    }),
    retrato: retrato(),
    depois: "typeof abrirPortaria === 'function' && abrirPortaria()",
  },

  /** A queda: a faixa sobre a sessão, com a contagem do período de graça. */
  queda: { resposta: respostas(), retrato: retrato({ link: { InternalBattery: { remaining_seconds: 214, attempts: 3 } } }) },

  /** O fim da sessão, que é outra tela e não uma faixa. */
  fim: { resposta: respostas(), retrato: retrato({ ended: "Kicked" }) },

  /** Um anexo recebido e um em envio, que é o par que a matriz pede. */
  anexos: {
    resposta: respostas({
      messages: [
        ...MENSAGENS,
        {
          id: 3, channel: 1, author: 2, author_nickname: "Lia", at_seconds: agora - 120,
          body: "", own: false, edited: false,
          attachment: { id: 11, file_name: "mapa-do-desvio.png", declared_type: "image/png", byte_size: 184320, expired: false },
        },
        {
          id: 4, channel: 1, author: 1, author_nickname: "Alex", at_seconds: agora - 30,
          body: "", own: true, edited: false,
          attachment: {
            id: 12,
            file_name: "rota-alternativa-pelo-norte-com-um-nome-bem-comprido.pdf",
            declared_type: "application/pdf", byte_size: 4194304, expired: false,
          },
        },
        {
          id: 5, channel: 1, author: 2, author_nickname: "Lia", at_seconds: agora - 10,
          body: "", own: false, edited: false,
          attachment: { id: 13, file_name: "nota-antiga.txt", declared_type: "text/plain", byte_size: 512, expired: true },
        },
      ],
    }),
    retrato: retrato(),
    // A conversa é buscada por um comando à parte quando a revisão muda; a
    // bancada a pede na mão porque não há laço de 500 ms aqui.
    depois: "sincronizarMensagens(99)",
  },
};

// ------------------------------------------------------------------ a corrida

async function principal() {
  const nome = process.argv[2] ?? "sessao";
  const saida = process.argv[3] ?? "/tmp/tela.png";
  const largura = Number(process.argv[4]) || 1280;
  const altura = Number(process.argv[5]) || 860;
  const cena = CENAS[nome];
  if (!cena) {
    console.error(`cena desconhecida: ${nome}. Há: ${Object.keys(CENAS).join(", ")}`);
    process.exit(1);
  }

  const { chromium } = require(process.env.PLAYWRIGHT
    ?? "/Users/dev-alexandre/SEELE-MOD-PERFIS/node_modules/playwright");
  const servidor = servir();
  await new Promise((ok) => servidor.listen(0, "127.0.0.1", ok));
  const url = `http://127.0.0.1:${servidor.address().port}/`;

  const navegador = await chromium.launch({ headless: true });
  const aba = await navegador.newPage({ viewport: { width: largura, height: altura } });
  const erros = [];
  aba.on("pageerror", (e) => erros.push(String(e.message).slice(0, 200)));
  aba.on("console", (m) => {
    if (m.type() === "error" || m.type() === "warning") erros.push(m.text().slice(0, 240));
  });

  // O retrato também responde ao comando `snapshot`: as camadas o pedem de
  // novo quando abrem, e sem ele a moderação sai sem desenhar.
  cena.resposta.snapshot = cena.retrato;
  await aba.addInitScript((tabela) => {
    const ouvintes = new Map();
    window.__seeleOuvintes = ouvintes;
    window.__TAURI__ = {
      core: {
        invoke: async (cmd, args) => {
          window.__seeleChamadas = window.__seeleChamadas ?? [];
          window.__seeleChamadas.push(cmd);
          if (Object.hasOwn(tabela, cmd)) return tabela[cmd];
          // **Nulo, e anotado.** Um comando sem resposta combinada não pode
          // virar exceção: metade da tela deixaria de desenhar e o retrato
          // mostraria um defeito que é da bancada.
          window.__seeleSemResposta = window.__seeleSemResposta ?? [];
          if (!window.__seeleSemResposta.includes(cmd)) window.__seeleSemResposta.push(cmd);
          return null;
        },
      },
      event: {
        listen: async (nome, fn) => {
          ouvintes.set(nome, fn);
          return () => ouvintes.delete(nome);
        },
      },
      window: { getCurrentWindow: () => ({ onCloseRequested: () => {}, isMaximized: async () => false }) },
    };
  }, cena.resposta);

  await aba.goto(url);
  await aba.waitForTimeout(700);

  // A sessão é a única tela que o produto não abre sozinho sem um `connect`.
  await aba.evaluate(() => {
    for (const secao of document.querySelectorAll("section.tela")) secao.hidden = true;
    document.getElementById("tela-sessao").hidden = false;
  });
  await aba.evaluate((r) => window.desenhar(r), cena.retrato)
    .catch((e) => erros.push("desenhar: " + e.message));
  await aba.waitForTimeout(400);
  if (cena.depois) {
    await aba.evaluate(cena.depois).catch((e) => erros.push("depois: " + e.message));
  }
  await aba.waitForTimeout(600);

  await aba.screenshot({ path: saida });
  if (process.env.DIAG) {
    console.log("diag:", JSON.stringify(await aba.evaluate(() => ({
      linhas: document.querySelectorAll("#lista-mensagens > *").length,
      lista: document.getElementById("lista-mensagens")?.className ?? "sem #lista-mensagens",
      ids: [...document.querySelectorAll("[id]")].map(n => n.id).filter(i => /mensag|conversa/.test(i)),
      alturaDaLista: document.getElementById("lista-mensagens")?.clientHeight,
      porDesenhar: typeof mensagensPorDesenhar === "undefined" ? "?" : mensagensPorDesenhar,
      quantas: typeof mensagens === "undefined" ? "?" : (mensagens?.length ?? null),
      canalAberto: typeof canalAberto === "undefined" ? "?" : canalAberto,
    }))));
  }
  const semResposta = await aba.evaluate(() => window.__seeleSemResposta ?? []);
  console.log(`${nome} → ${saida} (${largura}×${altura})`);
  if (semResposta.length) console.log("  sem resposta combinada:", semResposta.join(", "));
  if (erros.length) console.log("  erros:", [...new Set(erros)].slice(0, 6).join(" · "));

  await navegador.close();
  servidor.close();
}

principal().catch((erro) => {
  console.error(erro);
  process.exitCode = 1;
});
