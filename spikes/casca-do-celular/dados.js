// Os dados de exemplo do protótipo.
//
// Nomes de campo do `Snapshot` de `seele-ffi` (os mesmos que o duble de
// `tools/carga-da-casca.py` usa), para a passagem do protótipo à casca ser uma
// troca de fonte e não de forma. O que o `Snapshot` não tem e o celular precisa
// (a capa de MOD, o apelido verificado ao lado do nome de MOD, o FORA DO AR)
// vem nos nomes do rascunho da API 6 e do protocolo da F1.

"use strict";

const P = (id, nickname, opcoes = {}) => ({
  id, nickname, speaking: false, muted: false, total_isolation: false,
  signal: 96, sync_band: "Verde", is_self: false, ...opcoes,
});

const QUADRO = {
  link_state: "Verified",
  server: "TÓQUIO-3",
  me: 1,
  nickname: "rafa",
  voice_rooms: [
    { id: 1, name: "PONTE", limit: 8, password_required: false, occupied_by_us: true,
      people: [P(1, "rafa", { is_self: true, signal: 92 }), P(2, "juli", { speaking: true, signal: 88 }),
               P(3, "vitor", { signal: 71, sync_band: "Laranja", muted: true })] },
    { id: 2, name: "OFICINA", limit: 6, password_required: false, occupied_by_us: false,
      people: [P(4, "dani", { signal: 54, sync_band: "Vermelho" })] },
    { id: 3, name: "COFRE", limit: 4, password_required: true, occupied_by_us: false, people: [] },
    { id: 4, name: "PALCO", limit: 2, password_required: false, occupied_by_us: false,
      people: [P(7, "lu", { signal: 90 }), P(8, "teo", { signal: 86 })] },
  ],
  // Quem está no servidor e fora de sala de voz.
  presentes_sem_sala: [P(5, "maia", { signal: 94 }), P(6, "bia", { signal: 63, sync_band: "Laranja" })],
  // O FORA DO AR (D-fio-4): só id e apelido, sem horário e sem estado.
  membros_fora_do_ar: [{ id: 20, nickname: "ana" }, { id: 21, nickname: "bruno" },
                       { id: 22, nickname: "caio" }, { id: 23, nickname: "duda" }],
  channels: [
    { id: 1, name: "geral", open: true, novas: 0 }, { id: 2, name: "obras", open: false, novas: 3 },
    { id: 3, name: "links", open: false, novas: 0 }, { id: 4, name: "ruido", open: false, novas: 12 },
  ],
  telemetry: { rtt_ms: 23, jitter_ms: 4, loss_fraction: 0.002, bitrate_bps: 48000, signal: 92 },
  muted: false, total_isolation: false,
  may_manage_voice_rooms: true, may_kick: true, may_customise_server: true,
  // Quem transmite a tela nesta sala.
  transmissoes: [{ pessoa: 2, nickname: "juli" }, { pessoa: 3, nickname: "vitor" }],
};

// O nome que um MOD desenha no lugar do apelido (D-nucleo-1): o verificado
// continua à vista ao lado.
const NOMES_DE_MOD = { 3: "Capitão Vitor" };

// Quem tem capa de MOD (D-capa-1). As capas do protótipo são desenho de
// exemplo, e não do produto: no produto elas vêm da metade de servidor do MOD.
const CAPAS = { 1: "a", 2: "b", 3: "c" };

const MENSAGENS = [
  { id: 1, author: 2, author_nickname: "juli", at: "22:54", body: "subi o seeled aqui de casa, a porta 8383 tá aberta no roteador" },
  { id: 2, author: 3, author_nickname: "vitor", at: "22:54", body: "UDP? levei meia hora pra descobrir que a regra que eu tinha escrito era TCP" },
  { id: 3, author: 1, author_nickname: "rafa", at: "22:55", own: true, body: "UDP. tá escrito no README, duas linhas depois do comando" },
  { id: 4, author: 2, author_nickname: "juli", at: "22:56", body: "o furo de NAT funcionou daqui pro celular do vitor, o encontro só apresentou os dois e esqueceu" },
  { id: 5, author: 4, author_nickname: "dani", at: "22:57", body: "entrei na oficina, alguém vem?",
    attachment: { nome: "planta-da-oficina.pdf", tamanho: "1,2 MB" } },
  { id: 6, author: 1, author_nickname: "rafa", at: "22:58", own: true, falhou: true, body: "já vou, só fecho aqui" },
];

const SERVIDORES_DO_APARELHO = [
  { id: "t3", sigla: "T3", nome: "TÓQUIO-3", endereco: "tq3.duckdns.org:8383", ultima: "hoje", versao: "1.0" },
  { id: "of", sigla: "OF", nome: "OFICINA", endereco: "192.168.0.14:8383", ultima: "07/10", versao: "1.0" },
];

// O que a `identidade-web.js` da F4 entrega à tela «ENTRAR EM» (decisão da
// orquestradora, 09/10): **sem nome de servidor** — antes de conectar, o nome é
// só o que o link diz, e quem escreveu o link escolhe o que ele diz. O que se
// mostra é o que se confere: o endereço e a impressão inteira.
const CONVITE = {
  endereco: "tq3.duckdns.org:8383",
  impressao: "3f9a77c10b2ed4a8915c6e0fa2b7d3c48e1f6a0b9c2d7e5f4a3b1c0d9e8f7a6b",
  versaoDoServidor: "1.0",
  valeAte: "22/10",
  persistente: true,
};

const MODS_DO_SERVIDOR = [
  { id: "seele/perfis", nome: "PERFIS", versao: "4.0.0", oficial: true,
    faz: "cartão, retrato e capa de cada pessoa; uma página de perfil", alcance: ["pessoas.lista", "pessoa.capa", "operador.barra"] },
  { id: "seele/mesa", nome: "MESA", versao: "4.0.0", oficial: true,
    faz: "uma mesa de jogo na chamada e uma página da campanha", alcance: ["chamada.palco", "servidor.navegacao"] },
  { id: "seele/estilo", nome: "ESTILO", versao: "4.0.0", oficial: true, tema: true,
    faz: "a aparência do servidor", alcance: ["servidor.aparencia"] },
];

// Os temas de teste. PAPEL e PROFUNDO são os conjuntos do ESTILO 3.1.0
// (`SEELE-MOD-ESTILO/cliente/main.js`, `PRESETS`).
//
// **HOSTIL** é o achado do §8 do spec-mestre: o acento igual ao fundo, que passa
// na conferência de hoje (`oTextoAlcancaOFundo` só mede texto sobre fundo) e
// apaga o MUDO apertado do desktop. É o caso real contra o núcleo.
//
// **CLARO** é um tema legítimo, que passa em todos os pares da API 6 (texto e
// apagado sobre fundo e painel, acento sobre fundo e painel), porque a 6 traz
// `modo: 'claro'`. Ele mede outra coisa: as cores que o produto escreve direto
// sobre uma superfície que o tema pinta.
const TEMAS = {
  papel: { fundo: "#12100e", painel: "#1b1815", texto: "#f2ece0", apagado: "#a2988a",
           acento: "#c9a227", borda: "#4a423a", densidade: "confortavel", fonte: "sans",
           arredondamento: 4, brilho: false },
  profundo: { fundo: "#03060b", painel: "#081019", texto: "#dbe7f2", apagado: "#7e94ab",
              acento: "#38bdf8", borda: "#1f3347", densidade: "compacta", fonte: "mono",
              arredondamento: 8, brilho: true },
  hostil: { fundo: "#050403", painel: "#0a0806", texto: "#eae3cf", apagado: "#908574",
            acento: "#050403", borda: "#241f19", densidade: "compacta", fonte: "mono",
            arredondamento: 0, brilho: false },
  claro: { fundo: "#f2ece0", painel: "#e6dfcf", texto: "#12100e", apagado: "#574e43",
           acento: "#a83a10", borda: "#b8ad9a", densidade: "confortavel", fonte: "mono",
           arredondamento: 4, brilho: false },
};
