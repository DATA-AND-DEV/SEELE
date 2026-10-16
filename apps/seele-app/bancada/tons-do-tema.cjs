// Os pares que o tema confere e os tons que o produto deriva dele (rascunho da
// API 6, §9; tarefa 17e do plano da F3).
//
// # Por que esta bancada existe
//
// O tema do MOD escolhe seis cores; o produto desenha com mais: o preenchimento
// do acento (`--seele-laranja-fraco`), as cores de estado (o fósforo de quem
// fala e da sincronia, o azul, o vermelho) e o preenchimento de cada uma. Com
// um tema claro, essas cores do produto sumiam sobre o painel claro (o «juli
// fala» em fósforo, medido pela F8). Aqui se prova a regra:
//
// - **o MOD responde pelas cores dele**: texto, apagado e acento sobre fundo e
//   painel, nos pisos do rascunho; um par abaixo recusa o tema, com o par;
// - **o produto responde pelas que deriva**: cada cor de estado mantém o matiz
//   e muda a luminosidade até alcançar 4,5:1 sobre o fundo, o painel e o
//   próprio preenchimento, e o preenchimento do acento se aproxima do fundo até
//   o acento alcançar 4,5:1 sobre ele. Só quando nenhuma luminosidade serve o
//   tema é recusado, pela cor de estado;
// - **sem tema, nada é derivado**, e o produto fica como é.

const fs = require("node:fs");
const path = require("node:path");
const vm = require("node:vm");

const repo = path.resolve(__dirname, "../../..");
const base = fs.readFileSync(`${repo}/apps/seele-app/ui/base.js`, "utf8");

const falhas = [];
function confere(caso, condicao, detalhe) {
  if (!condicao) falhas.push(`${caso}: ${detalhe}`);
}

/** Do começo do bloco até o fim da função `ultima`. */
function bloco(comeco, ultima) {
  const i = base.indexOf(comeco);
  const k = base.indexOf(ultima, i);
  if (i < 0 || k < 0) throw new Error(`não achei o bloco de ${comeco} até ${ultima} em ui/base.js`);
  const j = base.indexOf("\n}\n", k);
  return base.slice(i, j + 3);
}

const contexto = { JSON, Object, Array, Number, String, Math };
vm.createContext(contexto);
vm.runInContext(
  [
    "const COR_DO_TEMA = /^#[0-9a-f]{6}$/i;",
    "const TEMA_DA_API = Object.freeze({ fundo: '--seele-negro-absoluto', texto: '--seele-osso', acento: '--seele-laranja-nerv', borda: '--seele-linha', painel: '--seele-negro-painel', apagado: '--seele-rotulo-painel' });",
    bloco("const PARES_DO_TEMA", "function tonsDoTema("),
    // A luminância mora numa função só desde a 17i (o `modo` a usa também).
    "function luminanciaRelativa(cor) {" + base.split("function luminanciaRelativa(cor) {")[1].split("\n}\n")[0] + "\n}",
    "function contrasteEntre(a, b) {" + base.split("function contrasteEntre(a, b) {")[1].split("\n}\n")[0] + "\n}",
    "globalThis.__api = { tonsDoTema, contrasteEntre, oklabDe };",
  ].join("\n"),
  contexto,
);
const { tonsDoTema, contrasteEntre } = contexto.__api;

/**
 * As cores do produto, lidas de `tokens.css` — e não copiadas aqui: o atalho
 * das superfícies do padrão promete os tokens **exatos**, e uma cópia que
 * envelhecesse provaria a cópia.
 */
const PRODUTO = Object.fromEntries(
  [...fs.readFileSync(`${repo}/apps/seele-app/ui/tokens.css`, "utf8")
    .matchAll(/^\s*(--seele-[a-z0-9-]+):\s*(#[0-9a-fA-F]{6})\s*;/gm)]
    .map(([, token, cor]) => [token, cor.toLowerCase()]),
);
for (const token of ["--seele-negro-absoluto", "--seele-negro-painel", "--seele-osso", "--seele-laranja-nerv",
  "--seele-laranja-fraco", "--seele-fosforo", "--seele-fosforo-fraco", "--seele-padrao-azul", "--seele-azul-fraco",
  "--seele-vermelho-alerta", "--seele-vermelho-fraco", "--seele-osso-apagado"]) {
  if (!PRODUTO[token]) throw new Error(`${token} sumiu de tokens.css`);
}
const daRaiz = (token) => PRODUTO[token] ?? "";

// ---------------------------------------- CIEDE2000, para medir o «perto»

function lab(hex) {
  const lin = [1, 3, 5].map((i) => {
    const c = parseInt(hex.slice(i, i + 2), 16) / 255;
    return c <= 0.04045 ? c / 12.92 : ((c + 0.055) / 1.055) ** 2.4;
  });
  const [r, g, b] = lin;
  const X = (0.4124 * r + 0.3576 * g + 0.1805 * b) / 0.95047;
  const Y = 0.2126 * r + 0.7152 * g + 0.0722 * b;
  const Z = (0.0193 * r + 0.1192 * g + 0.9505 * b) / 1.08883;
  const f = (t) => (t > 0.008856 ? Math.cbrt(t) : 7.787 * t + 16 / 116);
  return [116 * f(Y) - 16, 500 * (f(X) - f(Y)), 200 * (f(Y) - f(Z))];
}
function de2000(a, b) {
  const [L1, a1, b1] = lab(a);
  const [L2, a2, b2] = lab(b);
  const rad = Math.PI / 180;
  const C1 = Math.hypot(a1, b1), C2 = Math.hypot(a2, b2), Cb = (C1 + C2) / 2;
  const G = 0.5 * (1 - Math.sqrt(Cb ** 7 / (Cb ** 7 + 25 ** 7)));
  const a1p = (1 + G) * a1, a2p = (1 + G) * a2;
  const C1p = Math.hypot(a1p, b1), C2p = Math.hypot(a2p, b2);
  const h1p = (Math.atan2(b1, a1p) / rad + 360) % 360, h2p = (Math.atan2(b2, a2p) / rad + 360) % 360;
  const dLp = L2 - L1, dCp = C2p - C1p;
  let dhp = h2p - h1p;
  if (C1p * C2p === 0) dhp = 0;
  else if (dhp > 180) dhp -= 360;
  else if (dhp < -180) dhp += 360;
  const dHp = 2 * Math.sqrt(C1p * C2p) * Math.sin((dhp / 2) * rad);
  const Lbp = (L1 + L2) / 2, Cbp = (C1p + C2p) / 2;
  let hbp = h1p + h2p;
  if (C1p * C2p !== 0) hbp = Math.abs(h1p - h2p) <= 180 ? hbp / 2 : (hbp < 360 ? (hbp + 360) / 2 : (hbp - 360) / 2);
  const T = 1 - 0.17 * Math.cos((hbp - 30) * rad) + 0.24 * Math.cos(2 * hbp * rad)
    + 0.32 * Math.cos((3 * hbp + 6) * rad) - 0.2 * Math.cos((4 * hbp - 63) * rad);
  const dth = 30 * Math.exp(-(((hbp - 275) / 25) ** 2));
  const Rc = 2 * Math.sqrt(Cbp ** 7 / (Cbp ** 7 + 25 ** 7));
  const Sl = 1 + (0.015 * (Lbp - 50) ** 2) / Math.sqrt(20 + (Lbp - 50) ** 2);
  const Sc = 1 + 0.045 * Cbp, Sh = 1 + 0.015 * Cbp * T, Rt = -Math.sin(2 * dth * rad) * Rc;
  return Math.sqrt((dLp / Sl) ** 2 + (dCp / Sc) ** 2 + (dHp / Sh) ** 2 + Rt * (dCp / Sc) * (dHp / Sh));
}

const CLARO = { fundo: "#f2ece0", painel: "#e6dfd0", texto: "#12100e", apagado: "#5a5148", acento: "#8a4a00", borda: "#b8ad9c" };
const ESTADO = [
  ["--seele-fosforo", "--seele-fosforo-fraco"],
  ["--seele-padrao-azul", "--seele-azul-fraco"],
  ["--seele-vermelho-alerta", "--seele-vermelho-fraco"],
];

// ---------------------------------------- sem tema, nada é derivado

{
  const r = tonsDoTema({}, daRaiz);
  confere("sem tema", Object.keys(r.tons).length === 0 && r.pares.length === 0,
    `sem tema, o produto derivou ou recusou alguma coisa: ${JSON.stringify(r)}`);
}

// ---------------------------------------- as superfícies exatas do padrão: os tokens do produto

{
  // Os `-fraco` do produto foram escolhidos à mão sobre estas superfícies, e
  // nenhuma fórmula os reproduz (ΔE2000 de 3,2 a 4,2, medido na 17e): com elas,
  // o tema recebe os tokens, ΔE 0.
  const padrao = {
    fundo: PRODUTO["--seele-negro-absoluto"], painel: PRODUTO["--seele-negro-painel"], texto: PRODUTO["--seele-osso"],
    apagado: "#7a7061", acento: PRODUTO["--seele-laranja-nerv"], borda: "#241f19",
  };
  const r = tonsDoTema(padrao, daRaiz);
  confere("padrão · aceito", r.pares.length === 0, `as cores do padrão foram recusadas: ${JSON.stringify(r.pares)}`);
  for (const token of ["--seele-laranja-fraco", "--seele-osso-apagado", ...ESTADO.flat()]) {
    confere("padrão · o token do produto", r.tons[token] === PRODUTO[token],
      `${token} saiu ${r.tons[token]}, e o do produto é ${PRODUTO[token]}`);
  }
  confere("padrão · o acento sobre o preenchimento", contrasteEntre(padrao.acento, r.tons["--seele-laranja-fraco"]) >= 4.5,
    `o acento do produto sobre o preenchimento do produto ficou abaixo de 4,5:1`);

  // As superfícies do padrão com outro acento: o preenchimento do acento é
  // derivado (é dele), e as cores de estado continuam as do produto.
  const outroAcento = { ...padrao, acento: "#2f8fe0" };
  const o = tonsDoTema(outroAcento, daRaiz);
  confere("superfícies do padrão, outro acento · preenchimento derivado",
    o.tons["--seele-laranja-fraco"] !== PRODUTO["--seele-laranja-fraco"]
      && contrasteEntre(outroAcento.acento, o.tons["--seele-laranja-fraco"]) >= 4.5,
    `o preenchimento de um acento azul saiu ${o.tons["--seele-laranja-fraco"]}`);
  for (const token of ESTADO.flat()) {
    confere("superfícies do padrão, outro acento · estado do produto", o.tons[token] === PRODUTO[token],
      `${token} saiu ${o.tons[token]}, e o do produto é ${PRODUTO[token]}`);
  }

  // A um passo das superfícies do padrão, a derivação vale, e o número medido
  // é o do contrato: as cores de estado iguais, os preenchimentos a ΔE2000 ≤ 4,5.
  const quase = { ...padrao, fundo: "#060403" };
  const q = tonsDoTema(quase, daRaiz);
  for (const token of ESTADO.map(([cor]) => cor)) {
    confere("quase o padrão · cor de estado igual", q.tons[token] === PRODUTO[token],
      `${token} derivado como ${q.tons[token]}, e o do produto é ${PRODUTO[token]}`);
  }
  for (const token of ["--seele-laranja-fraco", ...ESTADO.map(([, f]) => f)]) {
    const d = de2000(q.tons[token] ?? "#000000", PRODUTO[token]);
    confere("quase o padrão · perto do produto", q.tons[token] !== PRODUTO[token] && d <= 4.5,
      `${token} derivado como ${q.tons[token]}, a ΔE2000 ${d.toFixed(2)} do produto (${PRODUTO[token]})`);
  }
}

// ---------------------------------------- o tema claro: tudo alcança o piso

{
  const r = tonsDoTema(CLARO, daRaiz);
  confere("claro · aceito", r.pares.length === 0, `o tema claro foi recusado: ${JSON.stringify(r.pares)}`);
  for (const [cor, fraco] of ESTADO) {
    const c = r.tons[cor], f = r.tons[fraco];
    for (const [sobre, nome] of [[CLARO.fundo, "fundo"], [CLARO.painel, "painel"], [f, "preenchimento"]]) {
      const razao = c && sobre ? contrasteEntre(c, sobre) : 0;
      confere("claro · cor de estado", razao >= 4.5, `${cor} (${c}) sobre o ${nome} (${sobre}) dá ${razao.toFixed(2)}:1`);
    }
  }
  const fraco = r.tons["--seele-laranja-fraco"];
  const razao = fraco ? contrasteEntre(CLARO.acento, fraco) : 0;
  confere("claro · acento sobre o preenchimento", razao >= 4.5,
    `o acento (${CLARO.acento}) sobre o preenchimento (${fraco}) dá ${razao.toFixed(2)}:1`);
}

// ---------------------------------------- o segundo apagado do produto

{
  // Superfícies cinza-claro, onde o `#7a7061` do produto fica abaixo de 3:1.
  const medio = { fundo: "#b4b0aa", painel: "#bcb8b2", texto: "#000000", apagado: "#2a2622", acento: "#3a1a00" };
  const r = tonsDoTema(medio, daRaiz);
  const apagado = r.tons["--seele-osso-apagado"];
  for (const [sobre, nome] of [[medio.fundo, "fundo"], [medio.painel, "painel"]]) {
    const razao = apagado ? contrasteEntre(apagado, sobre) : 0;
    confere("segundo apagado", razao >= 3, `o segundo apagado (${apagado}) sobre o ${nome} dá ${razao.toFixed(2)}:1`);
  }
  const padrao = tonsDoTema({ fundo: "#050403" }, daRaiz);
  confere("segundo apagado · o do produto fica quando alcança",
    padrao.tons["--seele-osso-apagado"] === PRODUTO["--seele-osso-apagado"],
    `o segundo apagado mudou sobre as superfícies do produto: ${padrao.tons["--seele-osso-apagado"]}`);
}

// ---------------------------------------- uma cor que não carregou não vira recusa

{
  // A raiz sem o painel e sem as cores de estado: uma folha que não carregou.
  const metade = (token) => (token === "--seele-negro-absoluto" ? "#050403" : token === "--seele-osso" ? "#eae3cf" : "");
  const r = tonsDoTema({ acento: "#2f8fe0" }, metade);
  confere("sem cor medida", r.pares.length === 0,
    `uma cor da raiz que não carregou recusou o tema: ${JSON.stringify(r.pares)}`);
  confere("sem cor medida · nada inventado", !Object.values(r.tons).some((cor) => !/^#[0-9a-f]{6}$/.test(cor)),
    `um tom derivado de uma cor que não carregou: ${JSON.stringify(r.tons)}`);
}

// ---------------------------------------- os temas prontos do ESTILO 3.1.0 passam

for (const [nome, tema] of Object.entries({
  PAPEL: { fundo: "#12100e", painel: "#1b1815", texto: "#f2ece0", apagado: "#a2988a", acento: "#c9a227", borda: "#4a423a" },
  PROFUNDO: { fundo: "#03060b", painel: "#081019", texto: "#dbe7f2", apagado: "#7e94ab", acento: "#38bdf8", borda: "#1f3347" },
})) {
  const r = tonsDoTema(tema, daRaiz);
  confere(`ESTILO · ${nome}`, r.pares.length === 0, `um tema pronto do ESTILO passou a ser recusado: ${JSON.stringify(r.pares)}`);
}

// ---------------------------------------- as recusas, pelo par

{
  const r = tonsDoTema({ ...CLARO, painel: "#2a2622" }, daRaiz);
  const par = r.pares.find((p) => p.frente === "texto" && p.fundo === "painel");
  confere("recusa · texto sobre o painel", par && par.razao < 4.5 && par.piso === 4.5,
    `o texto escuro sobre um painel escuro não foi recusado pelo par: ${JSON.stringify(r.pares)}`);

  const cinza = tonsDoTema({ fundo: "#777777", painel: "#777777", texto: "#000000", apagado: "#000000", acento: "#000000" }, daRaiz);
  const estado = cinza.pares.find((p) => p.frente === "fosforo");
  confere("recusa · cor de estado sem luminosidade que sirva", Boolean(estado) && estado.piso === 4.5,
    `superfícies cinza-médio, onde nenhum fósforo alcança o piso, não recusaram pelo fósforo: ${JSON.stringify(cinza.pares)}`);
}

// ---------------------------------------- os pares, pelos nomes do contrato

{
  // **O segundo apagado, pelo nome que o `v6.json` dá ao derivado** (o achado
  // do ESTILO, 16/10): `tema.derivados-pelo-produto.nomes` diz
  // `apagadoSecundario`, e o par saía como `apagado-secundario`, que o MOD não
  // acha no contrato. Um fundo escuro e um painel claro: nenhuma luminosidade
  // dá 3:1 sobre os dois, nem acima, nem abaixo, nem entre eles.
  const v6 = JSON.parse(fs.readFileSync(`${repo}/api/v6.json`, "utf8"));
  const derivados = new Set(v6.tema["derivados-pelo-produto"].nomes);
  const largo = tonsDoTema({ fundo: "#404040", painel: "#aaaaaa", texto: "#000000", apagado: "#000000", acento: "#000000" }, daRaiz);
  const segundo = largo.pares.find((p) => p.piso === 3 && !["texto", "apagado", "acento"].includes(p.frente));
  confere("par · o segundo apagado sem luminosidade que sirva", Boolean(segundo),
    `as superfícies a 3:1 de nada não recusaram pelo segundo apagado: ${JSON.stringify(largo.pares)}`);
  confere("par · o segundo apagado pelo nome do contrato", Boolean(segundo) && derivados.has(segundo.frente),
    `o par do segundo apagado saiu como «${segundo?.frente}», e o v6.json o chama de ${[...derivados].join(" ou ")}`);
}

if (falhas.length) {
  console.error(`tons-do-tema: ${falhas.length} falha(s)\n  ${falhas.join("\n  ")}`);
  process.exit(1);
}
console.log("tons-do-tema: ok");
