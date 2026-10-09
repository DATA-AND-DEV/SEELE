// O retrato que um MOD da API 6 lê, projetado por lista branca.
//
// # Por que esta bancada existe
//
// Até a API 5 o MOD lê o `Snapshot` inteiro (`SeeleMods.snapshot`, sem filtro):
// o veredito de conexão segura (`link_state`), a topologia da rede (`caminho`),
// os nomes dos aparelhos da máquina (`capture`, `playback`), a telemetria. Na
// API 6 ele lê `SeeleUI.retrato()`, com os campos da tabela do rascunho da API 6,
// §6, e só esses (tarefa 4 do plano da F3).
//
// # Pela forma, e não por uma lista do que se proíbe
//
// Uma lista do que se proíbe envelhece calada: o campo novo que ninguém pôs nela
// passa. O I-b da F5 acrescentou `telemetry.microfone_entrega_silencio_digital`,
// o estado de uma permissão de privacidade do sistema, e é o caso que esta
// bancada carrega. O que se confere aqui é a forma: as chaves do retrato em cada
// nível são **exatamente** as da tabela, e um `Snapshot` com um campo
// desconhecido em cada nível sai sem nenhum deles.

const fs = require("node:fs");
const path = require("node:path");
const vm = require("node:vm");

const repo = path.resolve(__dirname, "../../..");
const base = fs.readFileSync(`${repo}/apps/seele-app/ui/base.js`, "utf8");

const falhas = [];
function confere(caso, condicao, detalhe) {
  if (!condicao) falhas.push(`${caso}: ${detalhe}`);
}

/** Do início da função até o primeiro "\n}\n" depois dele. */
function trecho(fonte, comeco) {
  const i = fonte.indexOf(comeco);
  if (i < 0) throw new Error(`não achei ${comeco} em ui/base.js`);
  const j = fonte.indexOf("\n}\n", i);
  return fonte.slice(i, j + 3);
}

const contexto = { JSON, Object, Array, Number, String };
vm.createContext(contexto);
vm.runInContext(
  `${trecho(base, "function projetarORetrato(")}\nglobalThis.projetarORetrato = projetarORetrato;`,
  contexto,
);
const { projetarORetrato } = contexto;

/** Uma pessoa como `seele_ffi::types::Person` a serializa. */
const pessoa = (id, nickname, extra = {}) => ({
  id, nickname, speaking: false, muted: false, total_isolation: false,
  signal: 100, sync_band: "Nominal", is_self: false, ...extra,
});

/** Um `Snapshot` com os campos de `crates/seele-ffi/src/types.rs`. */
const snapshotCompleto = {
  caminho: "rede-local",
  link: "Conectado",
  link_state: "ConexaoSegura",
  server: "Casa",
  icon_revision: 3,
  person_icons_revision: 2,
  me: 11,
  nickname: "ana",
  voice_rooms: [
    {
      id: 1, name: "Mesa", limit: 6, password_required: true, occupied_by_us: true,
      channel: 7, people: [pessoa(11, "ana", { is_self: true }), pessoa(12, "juli", { speaking: true })],
      sync: { ratio: 98, band: "Nominal", people: 2 },
    },
    { id: 2, name: "Recepção", limit: 0, password_required: false, occupied_by_us: false, channel: null, people: [], sync: null },
  ],
  presentes: [
    pessoa(11, "ana", { is_self: true }),
    pessoa(12, "juli", { speaking: true }),
    pessoa(13, "bia", { muted: true }),
  ],
  channels: [
    { id: 6, name: "geral", open: false },
    { id: 7, name: "mesa", open: true },
  ],
  messages_revision: 41,
  telemetry: { rtt_ms: 18, jitter_ms: 2, codec: "opus", microfone_entrega_silencio_digital: true },
  notice: { kind: "Aviso", text: "algo" },
  muted: false,
  total_isolation: false,
  speaking: false,
  voice_mode: "Aberta",
  audio_available: true,
  abertura_do_audio: "Aberta",
  capture: { name: "MacBook Pro Microphone" },
  playback: { name: "AirPods de Ana" },
  aparelho: "Pronto",
  trocas_de_aparelho: 1,
  tela: { id: 4 },
  minha_transmissao: null,
  transmissoes: [{ id: 4 }],
  may_manage_voice_rooms: true,
  may_kick: true,
  may_ban: false,
  may_remove_message: true,
  may_move_person: false,
  may_customise_server: true,
  may_delete_rooms: false,
};
const casca = { onde: "computador", regioes: ["canal.cabecalho", "pagina"] };

// ------------------------------------------------- passo 1 · a lista branca

const r = projetarORetrato(snapshotCompleto, casca);
const texto = JSON.stringify(r);
for (const proibido of [
  "link_state", "link", "caminho", "capture", "playback", "aparelho", "trocas_de_aparelho",
  "telemetry", "notice", "tela", "transmissoes", "voice_mode",
  "microfone_entrega_silencio_digital", "may_kick", "is_self", "sync_band", "password_required",
]) {
  confere("lista branca", !(proibido in r) && !texto.includes(`"${proibido}"`),
    `o retrato leva «${proibido}»: ${texto}`);
}
confere("canal aberto", r.canais?.find?.((c) => c.aberto)?.id === 7,
  `o canal aberto é o que tem \`open\`: ${JSON.stringify(r.canais)}`);
confere("sem open_channel", !("open_channel" in r), "`open_channel` nunca existiu (MESA A1)");

// ------------------------------------------------- os valores, campo a campo

confere("eu", r.eu === 11, `\`eu\` é o \`me\`: ${r.eu}`);
confere("servidor", r.servidor?.nome === "Casa", `\`servidor.nome\` é o \`server\`: ${JSON.stringify(r.servidor)}`);
confere("pessoas", JSON.stringify(r.pessoas) === JSON.stringify([
  { id: 11, apelido: "ana", falando: false, mudo: false },
  { id: 12, apelido: "juli", falando: true, mudo: false },
  { id: 13, apelido: "bia", falando: false, mudo: true },
]), `as pessoas são as presentes, com os nomes da API: ${JSON.stringify(r.pessoas)}`);
confere("salas", JSON.stringify(r.salas) === JSON.stringify([
  { id: 1, nome: "Mesa", limite: 6, pessoas: [11, 12], minha: true },
  { id: 2, nome: "Recepção", limite: 0, pessoas: [], minha: false },
]), `as salas levam os ids de quem está nelas, e não as pessoas inteiras: ${JSON.stringify(r.salas)}`);
confere("onde", r.onde === "computador", `\`onde\` vem da casca: ${r.onde}`);
confere("regioes", JSON.stringify(r.regioes) === JSON.stringify(casca.regioes),
  `\`regioes\` vem da casca: ${JSON.stringify(r.regioes)}`);
confere("regioes é cópia", r.regioes !== casca.regioes,
  "o retrato devolve a lista da casca, e não uma cópia: quem a mexer mexe na do produto");

// ------------------------------------------- passo 4b · pela forma, em cada nível

const comCampoNovo = JSON.parse(JSON.stringify(snapshotCompleto));
comCampoNovo.campo_novo_no_topo = "segredo-0";
comCampoNovo.telemetry.campo_novo_na_telemetria = "segredo-1";
comCampoNovo.presentes[0].campo_novo_na_pessoa = "segredo-2";
comCampoNovo.voice_rooms[0].campo_novo_na_sala = "segredo-3";
comCampoNovo.voice_rooms[0].people[0].campo_novo_na_pessoa_da_sala = "segredo-4";
comCampoNovo.channels[0].campo_novo_no_canal = "segredo-5";
const novo = projetarORetrato(comCampoNovo, casca);
const textoNovo = JSON.stringify(novo);
confere("campos novos", !/segredo-\d|campo_novo/.test(textoNovo),
  `um campo novo do Snapshot chegou ao retrato: ${textoNovo}`);

const chaves = (objeto) => JSON.stringify(Object.keys(objeto ?? {}));
const exatas = (caso, objeto, esperadas) =>
  confere(`forma · ${caso}`, chaves(objeto) === JSON.stringify(esperadas),
    `as chaves são ${chaves(objeto)}, e a tabela do rascunho §6 diz ${JSON.stringify(esperadas)}`);
exatas("o topo", novo, ["eu", "servidor", "pessoas", "salas", "canais", "onde", "regioes"]);
exatas("servidor", novo.servidor, ["nome"]);
for (const p of novo.pessoas ?? []) exatas("pessoa", p, ["id", "apelido", "falando", "mudo"]);
for (const s of novo.salas ?? []) exatas("sala", s, ["id", "nome", "limite", "pessoas", "minha"]);
for (const c of novo.canais ?? []) exatas("canal", c, ["id", "nome", "aberto"]);

// ------------------------------------------- um snapshot de antes do `Session`

let vazio;
try {
  vazio = projetarORetrato({}, { onde: "celular", regioes: [] });
} catch (erro) {
  confere("antes do Session", false, `um Snapshot sem as listas lançou: ${erro}`);
}
if (vazio) {
  confere("antes do Session", vazio.eu === null && vazio.servidor?.nome === ""
    && vazio.pessoas?.length === 0 && vazio.salas?.length === 0 && vazio.canais?.length === 0
    && vazio.onde === "celular",
  `um Snapshot sem as listas não deu o retrato vazio: ${JSON.stringify(vazio)}`);
}

if (falhas.length) {
  console.error(`retrato-projetado: ${falhas.length} falha(s)\n  ${falhas.join("\n  ")}`);
  process.exit(1);
}
console.log("retrato-projetado: ok");
