// O que ninguém pegou vai ao `seele.log`, e a tela não muda.
//
// O botão vermelho não fechava o app: o `destroy()` da janela era recusado
// pela ACL do Tauri, a recusa caía numa promise que ninguém olhava, e nada
// chegava ao `seele.log` (achado da F7). Este roteiro afirma o que o
// `registrar_da_janela` recebe (`onde=sem-dono`, em `base.js`):
//
// 1. a promise recusada sem `catch`, com o motivo — uma cadeia, como a da ACL —
//    e, quando o motivo é um erro, com a pilha;
// 2. o erro solto, com a pilha; e o solto que não é erro, com o lugar;
// 3. a tela fica como estava;
// 4. **o teto**: um laço que recusa não passa de `SEM_DONO.porMinuto` linhas
//    no minuto, e as seguradas são contadas e ditas numa linha só, quando o
//    minuto acaba.
//
// Os erros daqui não se chamam `…Error`: o aparelho conta como estouro todo
// `Uncaught … Error:` do console, e o que se afirma aqui é o registro.

const doRegistro = () =>
  window.__SEELE_HISTORICO
    .filter(([cmd, args]) => cmd === "registrar_da_janela" && args && args.onde === "sem-dono")
    .map(([, args]) => args);
// Sem aspas duplas no que se relata: o aparelho lê o relato do console até a
// primeira delas.
const dito = (valor) => (JSON.stringify(valor) ?? String(valor)).replaceAll('"', "'");
const comFrase = (pedaco) => doRegistro().find((args) => String(args.oQue).includes(pedaco));
const antes = telas("antes");

// 1. A recusa da ACL, e uma recusa com pilha.
Promise.reject("window.destroy not allowed. Permissions associated with this command: core:window:allow-destroy");
Promise.reject(Object.assign(new Error("a recusa com pilha"), { name: "RecusaDaBancada" }));
// 2. O erro solto, e o solto que não é erro.
setTimeout(() => {
  throw Object.assign(new Error("o erro solto da bancada"), { name: "EstouroDaBancada" });
}, 0);
setTimeout(() => {
  throw "a cadeia solta da bancada";
}, 0);
await ate(() => doRegistro().length >= 4, 1500);

const acl = comFrase("core:window:allow-destroy");
relatar("a recusa da ACL: " + dito(acl));
exigir(acl, "a promise recusada sem catch não chegou ao registro: " + dito(doRegistro()));
exigir(acl.nivel === "erro" && acl.oQue.startsWith("promise recusada sem catch:"),
  "a recusa chegou ao registro sem o nível ou sem dizer o que é: " + dito(acl));
const comPilha = comFrase("RecusaDaBancada: a recusa com pilha");
exigir(comPilha && comPilha.oQue.includes("— pilha: "),
  "a recusa que é um erro chegou sem a pilha: " + dito(comPilha));
const solto = comFrase("EstouroDaBancada: o erro solto da bancada");
relatar("o erro solto: " + dito(solto?.oQue));
exigir(solto && solto.oQue.startsWith("erro solto:") && solto.oQue.includes("— pilha: "),
  "o erro solto não chegou ao registro com a pilha: " + dito(solto));
const cadeia = comFrase("a cadeia solta da bancada");
exigir(cadeia && /index\.html:\d+:\d+\)$/.test(cadeia.oQue),
  "o solto que não é erro chegou sem o lugar: " + dito(cadeia));

// 3. A tela como estava.
exigir(telas("antes") === antes, "a tela mudou com o que ninguém pegou: " + telas("depois"));

// 4. O teto. O fim do minuto é pego aqui, em vez de esperado: o aparelho dá
// à página cinco segundos de relógio.
const fimDoMinuto = [];
const esperarDeVerdade = window.setTimeout;
window.setTimeout = (funcao, ms, ...resto) =>
  ms > 10_000 ? (fimDoMinuto.push({ funcao, ms }), 0) : esperarDeVerdade(funcao, ms, ...resto);
try {
  for (let i = 0; i < 30; i++) Promise.reject(`a recusa do laço, volta ${i}`);
  await espera(400);
} finally {
  window.setTimeout = esperarDeVerdade;
}
relatar(`no laço: ${doRegistro().length} linhas no minuto, ${fimDoMinuto.length} fim de minuto marcado`);
exigir(doRegistro().length === SEM_DONO.porMinuto,
  `o laço deixou ${doRegistro().length} linhas no minuto, e o teto é ${SEM_DONO.porMinuto}`);
exigir(fimDoMinuto.length === 1 && fimDoMinuto[0].ms > 0 && fimDoMinuto[0].ms <= SEM_DONO.minuto,
  "a conta das seguradas não ficou marcada para o fim do minuto: " + dito(fimDoMinuto.map((f) => f.ms)));
fimDoMinuto[0].funcao();
const conta = doRegistro().at(-1);
relatar("no fim do minuto: " + conta.oQue);
const seguradas = Number(/^(\d+) outras seguradas/.exec(conta.oQue)?.[1]);
exigir(seguradas >= 24, "a conta das seguradas não foi dita, ou não contou o laço: " + dito(conta));
