// SEELE — as portas de jogo abertas (`#tuneis`), na casca (API 8, parte 2).
//
// # O que este arquivo desenha
//
// Uma faixa **do produto**, fora de toda superfície de MOD, com uma linha por
// túnel de jogo de quem joga:
//
//   porta 25565 aberta para minecraft-1 (x/jogo) · ligação direta com Fulano · 34 ms · 120 KB/s
//
// A porta em que o jogo deve ligar, o nome do túnel e o MOD que o pediu; por
// onde o tráfego anda (a ligação direta nomeia a pessoa da máquina, a estrela
// diz que passa pelo anfitrião); a latência e a vazão; a porta trocada, quando a
// do jogo estava ocupada; e a última queda, com o motivo em frase.
//
// # Por que fora das superfícies de MOD
//
// Um MOD **pede** a porta (`SeeleUI.tunel.entrar`, de um clique), mas quem diz
// que ela está aberta é o produto. Uma faixa que um MOD pudesse desenhar, calar
// ou cobrir seria uma porta aberta na máquina de quem joga sem que a pessoa
// soubesse. Nada daqui passa por `mods-regiao.js`.
//
// # De onde vem o que ela mostra
//
// - `tuneis_abertos`, ao carregar e quando a sessão da vista muda
//   (`seele-tuneis-reler`): o que já estava de pé;
// - `TunnelReport`, no `seele://event`: o relatório de cada túnel; o último de
//   um túnel traz `encerrado` com o motivo, e a linha vira «fechou: …», com um
//   DISPENSAR;
// - os eventos da casca (`base.js`): `seele-tunel-aberto` (a porta acabou de
//   abrir), `seele-tunel-recusado` (a entrada foi recusada, e por quê),
//   `seele-tunel-mod-saiu` (o MOD saiu; a linha sai junto, **se o Rust fechou**)
//   e `seele-tuneis-limpar` (os MODs da vista saíram: fim ou troca de vista).
//
// **Nada aqui engole uma falha**: o que não vem é dito ao `seele.log`.

"use strict";

/** As linhas de túnel, por `mod\u0000nome`; a recusa tem o prefixo `recusa\u0000`. */
const tuneisNaFaixa = new Map();

/** A frase de um motivo, ou o próprio nome quando a lista não o conhece (e isso é dito). */
function fraseDoTunel(lista, nome) {
  const frase = FRASES_DO_TUNEL?.[lista]?.[nome];
  if (typeof frase === "string") return frase;
  registrarNoAnfitriao("tunel", `a faixa não tem frase para «${nome}» (${lista})`, "aviso");
  return String(nome);
}

/** `120 KB/s`, `1,5 KB/s`, `3 B/s`: a vazão como se lê de relance. */
function vazaoEmTexto(bytesPorSegundo) {
  const bytes = Number(bytesPorSegundo) || 0;
  if (bytes < 1024) return `${bytes} B/s`;
  const kb = bytes / 1024;
  if (kb < 1024) return `${kb < 10 ? kb.toFixed(1).replace(".", ",") : Math.round(kb)} KB/s`;
  return `${(kb / 1024).toFixed(1).replace(".", ",")} MB/s`;
}

/** Por onde o tráfego anda, em frase: nomeia a pessoa quando a ligação é direta. */
function caminhoEmTexto(caminho) {
  if (caminho?.tipo === "direto") {
    return caminho.apelido ? `ligação direta com ${caminho.apelido}` : "ligação direta";
  }
  return "pela estrela (via o anfitrião)";
}

function elementoDaFaixa(tag, classe, texto) {
  const el = document.createElement(tag);
  if (classe) el.className = classe;
  if (texto !== undefined) el.textContent = texto;
  return el;
}

/** Quantas vezes, em palavra. */
function vezesEmTexto(vezes) {
  return vezes === 1 ? "1 vez" : `${vezes} vezes`;
}

function chaveDoTunel(modId, nome) {
  return `${modId}\u0000${nome}`;
}

/** Guarda o relatório e acha a queda que subiu desde o anterior, se subiu. */
function aplicarORelatorio(entrada, relatorio) {
  entrada.relatorio = relatorio;
  entrada.porta = relatorio.porta;
  entrada.preferida = relatorio.preferida;
  entrada.desejada = relatorio.desejada;
  for (const { motivo, vezes } of relatorio.quedas ?? []) {
    const visto = entrada.vistas.get(motivo) ?? 0;
    // «Fechada» é uma conexão que terminou como devia, e não uma queda.
    if (vezes > visto && motivo !== "fechado") entrada.ultimaQueda = { motivo, vezes };
    entrada.vistas.set(motivo, vezes);
  }
  if (relatorio.encerrado) {
    entrada.estado = "fechada";
    entrada.motivo = relatorio.encerrado;
  }
}

function entradaNova(modId, nome, estado) {
  return {
    modId, nome, estado,
    porta: 0, preferida: true, desejada: 0,
    relatorio: null, ultimaQueda: null, vistas: new Map(),
    motivo: null, naoFechou: null,
  };
}

/** A linha de um túnel aberto, ou o bloco de um que fechou ou foi recusado. */
function desenharUmTunel(entrada) {
  const bloco = elementoDaFaixa("div", "tunel-bloco");
  bloco.dataset.estado = entrada.estado;
  bloco.dataset.mod = entrada.modId;
  bloco.dataset.nome = entrada.nome;
  const linha = elementoDaFaixa("p", "tunel-linha");
  const dono = `${entrada.nome} (${entrada.modId})`;

  if (entrada.estado === "recusada") {
    linha.append(
      elementoDaFaixa("span", "tunel-porta", "porta fechada"),
      ` — ${entrada.modId} não abriu ${entrada.nome}: ${fraseDoTunel("recusas", entrada.motivo)}`,
    );
  } else if (entrada.estado === "fechada") {
    linha.append(
      elementoDaFaixa("span", "tunel-porta", `porta ${entrada.porta}`),
      ` de ${dono} fechou: ${FRASES_DO_TUNEL.fins[entrada.motivo] ?? fraseDoTunel("quedas", entrada.motivo)}`,
    );
  } else {
    linha.append(
      elementoDaFaixa("span", "tunel-porta", `porta ${entrada.porta}`),
      ` aberta para ${dono}`,
    );
    const r = entrada.relatorio;
    const partes = [
      caminhoEmTexto(r?.caminho),
      r ? (r.latencia_ms === null ? "medindo…" : `${r.latencia_ms} ms`) : "medindo…",
      vazaoEmTexto(r?.bytes_por_segundo ?? 0),
    ];
    for (const parte of partes) linha.append(` · ${parte}`);
  }
  bloco.append(linha);

  // A porta trocada, dita com o número que a pessoa precisa usar.
  if (entrada.estado === "aberta" && entrada.preferida === false) {
    const queria = entrada.desejada ? `a ${entrada.desejada}` : "a porta do jogo";
    bloco.append(elementoDaFaixa("p", "tunel-nota", `${queria} estava ocupada; use localhost:${entrada.porta}`));
  }
  if (entrada.ultimaQueda) {
    const { motivo, vezes } = entrada.ultimaQueda;
    bloco.append(elementoDaFaixa(
      "p", "tunel-nota",
      `última queda: ${fraseDoTunel("quedas", motivo)} (${vezesEmTexto(vezes)})`,
    ));
  }
  if (entrada.naoFechou) {
    bloco.append(elementoDaFaixa(
      "p", "tunel-nota",
      `não consegui fechar esta porta: ${fraseDoTunel("recusas", entrada.naoFechou)}`,
    ));
  }
  if (entrada.estado !== "aberta") {
    const dispensar = elementoDaFaixa("button", "tunel-dispensar", "DISPENSAR");
    dispensar.type = "button";
    dispensar.addEventListener("click", () => {
      for (const [chave, outra] of tuneisNaFaixa) if (outra === entrada) tuneisNaFaixa.delete(chave);
      desenharOsTuneis();
    });
    bloco.append(dispensar);
  }
  return bloco;
}

function desenharOsTuneis() {
  const faixa = document.getElementById("tuneis");
  if (!faixa) return;
  faixa.replaceChildren(...[...tuneisNaFaixa.values()].map(desenharUmTunel));
  faixa.hidden = tuneisNaFaixa.size === 0;
}

function dizerAFalhaDaFaixa(oQue, falha) {
  registrarNoAnfitriao("tunel", `${oQue} — ${motivoDoTunel(falha)}`, "aviso");
}

/** Relê o que está de pé nesta sessão (`tuneis_abertos`) no lugar do que havia. */
async function relerOsTuneis() {
  tuneisNaFaixa.clear();
  desenharOsTuneis();
  let abertos;
  try {
    abertos = await invoke("tuneis_abertos");
  } catch (falha) {
    dizerAFalhaDaFaixa("tuneis_abertos falhou", falha);
    return;
  }
  for (const relatorio of Array.isArray(abertos) ? abertos : []) {
    const entrada = entradaNova(relatorio.mod_id, relatorio.nome, "aberta");
    aplicarORelatorio(entrada, relatorio);
    tuneisNaFaixa.set(chaveDoTunel(relatorio.mod_id, relatorio.nome), entrada);
  }
  desenharOsTuneis();
}

listen("seele://event", ({ payload }) => {
  const relatorio = payload?.TunnelReport?.report;
  if (!relatorio) return;
  const chave = chaveDoTunel(relatorio.mod_id, relatorio.nome);
  let entrada = tuneisNaFaixa.get(chave);
  // Sem linha, ou com a linha de uma porta que fechou e agora reabriu: uma
  // linha nova, de porta aberta.
  if (!entrada || (entrada.estado === "fechada" && !relatorio.encerrado)) {
    entrada = entradaNova(relatorio.mod_id, relatorio.nome, "aberta");
    tuneisNaFaixa.set(chave, entrada);
  }
  aplicarORelatorio(entrada, relatorio);
  desenharOsTuneis();
});

globalThis.addEventListener("seele-tunel-aberto", ({ detail }) => {
  const chave = chaveDoTunel(detail.modId, detail.nome);
  // **Junta, e não troca**: um relatório que chegou antes de o `invoke` voltar
  // já trouxe o caminho e a latência, e a linha nova os perderia.
  let entrada = tuneisNaFaixa.get(chave);
  if (!entrada || entrada.estado !== "aberta") {
    entrada = entradaNova(detail.modId, detail.nome, "aberta");
    tuneisNaFaixa.set(chave, entrada);
  }
  entrada.porta = detail.porta;
  entrada.preferida = detail.preferida;
  entrada.desejada = detail.desejada ?? 0;
  desenharOsTuneis();
});

globalThis.addEventListener("seele-tunel-recusado", ({ detail }) => {
  // **Uma linha de recusa por MOD, trocada e não somada**: a recusa mais nova
  // vale, e a faixa não cresce com o que um MOD pedir. O nome já veio conferido
  // (`nomeDeTunelValido`), e o motivo é um nome da lista.
  const entrada = entradaNova(detail.modId, detail.nome, "recusada");
  entrada.motivo = String(detail.motivo).slice(0, 40);
  tuneisNaFaixa.set(`recusa\u0000${detail.modId}`, entrada);
  desenharOsTuneis();
});

globalThis.addEventListener("seele-tunel-mod-saiu", ({ detail }) => {
  for (const [chave, entrada] of [...tuneisNaFaixa]) {
    if (entrada.modId !== detail.modId) continue;
    if (detail.fechou) tuneisNaFaixa.delete(chave);
    // O que o Rust não fechou continua à vista, com o motivo: a porta segue aberta.
    else if (entrada.estado === "aberta") entrada.naoFechou = detail.motivo;
  }
  desenharOsTuneis();
});

globalThis.addEventListener("seele-tuneis-limpar", () => {
  tuneisNaFaixa.clear();
  desenharOsTuneis();
});

globalThis.addEventListener("seele-tuneis-reler", () => {
  relerOsTuneis();
});

relerOsTuneis();
