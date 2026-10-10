// M3 da 0.16 — a metade de servidor do MOD de carga sintético da API 6.
//
// Não é um MOD de produto: a preparação (`gerar-carga.py`) a copia para cada
// variante do pacote e escreve aqui os dois GIFs, em base64. Ela atende três
// pedidos da janela:
//
// - `{ capa: 'animada' | 'parada' }`: a imagem de uma `pessoa.capa`, num
//   pedaço só (`{ bytes }`), como a janela lê a mídia do servidor;
// - `{ aviso: n }`: um `mundo.avisar` com 1 KiB de carga, para a janela pedir
//   no ritmo do teto (4 por segundo por MOD; rascunho §7.3);
// - qualquer outro: `{ ok: true }`.

const GIF_ANIMADO = "__GIF_ANIMADO__";
const GIF_PARADO = "__GIF_PARADO__";

// 1 KiB de carga, o teto do aviso (rascunho §7.3): a carga é o texto
// serializado, e as aspas contam.
const CARGA_DO_AVISO = "x".repeat(1024 - 2);

globalThis.aoPedir = (contexto, texto) => {
  let pedido = {};
  try {
    pedido = JSON.parse(texto);
  } catch {
    return JSON.stringify({ ok: false, erro: "pedido-ilegivel" });
  }
  if (pedido.capa === "animada") return JSON.stringify({ bytes: GIF_ANIMADO });
  if (pedido.capa === "parada") return JSON.stringify({ bytes: GIF_PARADO });
  if (typeof pedido.aviso === "number") {
    mundo.avisar({ carga: CARGA_DO_AVISO });
    return JSON.stringify({ ok: true, aviso: pedido.aviso });
  }
  return JSON.stringify({ ok: true });
};
