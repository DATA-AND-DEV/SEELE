// **A identidade fixa de uma página, só na bancada** (E6 da 0.17, pedido do
// coordenador para a E2E de cargos).
//
// O dono de um servidor é declarado pela impressão da chave dele
// (`--proprietario <hex64>`, ADR 0064), ANTES de o servidor aceitar a primeira
// conexão. Na página de produto a chave nasce na primeira entrada
// (`nascerAChave`, em `identidade-web.js`: Ed25519, a privada não exportável),
// tarde demais para a bancada saber a impressão antes de subir o servidor.
//
// Este módulo resolve isso SEM tocar no produto: um script de início
// (`contexto.addInitScript`) que roda antes dos scripts da página e troca, só
// naquele contexto do navegador de teste, o `crypto.subtle.generateKey` do
// Ed25519 por um que devolve sempre o MESMO par — nascido uma vez pelo
// `generateKey` de verdade, com os mesmos parâmetros do produto (a privada
// continua não exportável), e guardado num banco próprio da bancada
// (`bancada-identidade`), para a mesma página manter a mesma pessoa depois de
// recarregar. A impressão (o `key_fingerprint` do servidor: o SHA-256 da chave
// pública crua, em hex) fica em `window.__impressaoDaBancada`.
//
// O que NÃO muda: o produto não lê nada disto, não há chave nem opção nova na
// página, e o servidor confere a assinatura da chave como sempre. A página
// fixa a identidade dela; não se passa por ninguém.
//
// # Uso
//
//   const { fixarAIdentidade, impressaoDaPagina } = require("./identidade-da-bancada.cjs");
//   const contexto = await browser.newContext();   // um por pessoa
//   await fixarAIdentidade(contexto);
//   const aba = await contexto.newPage();
//   await aba.goto(url);
//   const dona = await impressaoDaPagina(aba);    // hex64, para --proprietario
//
// Cada contexto é uma pessoa: o banco do navegador é por contexto. As outras
// pessoas (as «conhecidas») entram uma vez no servidor, com a identidade fixa
// delas, e passam a existir no quadro e no FORA DO AR.

"use strict";

/** Roda NA PÁGINA, antes dos scripts dela. */
function instalarNaPagina() {
  if (window.__impressaoDaBancada) return;
  // O `iframe` opaco dos MODs (origem `null`) não tem IndexedDB, e a chave da
  // pessoa não mora nele: só a página troca o `generateKey`.
  // (`window.origin`, e não `location.origin`: o segundo é a origem da URL.)
  if (window.origin === "null") return;
  const sutil = crypto.subtle;
  const original = sutil.generateKey.bind(sutil);
  const NOME = "bancada-identidade";
  const abrir = () =>
    new Promise((ok, falha) => {
      const pedido = indexedDB.open(NOME, 1);
      pedido.onupgradeneeded = () => pedido.result.createObjectStore("chaves");
      pedido.onsuccess = () => ok(pedido.result);
      pedido.onerror = () => falha(pedido.error);
    });
  const par = (async () => {
    const banco = await abrir();
    try {
      const guardado = await new Promise((ok, falha) => {
        const pedido = banco.transaction("chaves", "readonly").objectStore("chaves").get("fixa");
        pedido.onsuccess = () => ok(pedido.result ?? null);
        pedido.onerror = () => falha(pedido.error);
      });
      if (guardado) return guardado;
      // Os mesmos parâmetros do `nascerAChave` do produto.
      const novo = await original({ name: "Ed25519" }, false, ["sign", "verify"]);
      await new Promise((ok, falha) => {
        const transacao = banco.transaction("chaves", "readwrite");
        transacao.objectStore("chaves").put(novo, "fixa");
        transacao.oncomplete = () => ok();
        transacao.onerror = () => falha(transacao.error);
      });
      return novo;
    } finally {
      banco.close();
    }
  })();
  let entregues = 0;
  sutil.generateKey = function generateKeyDaBancada(algoritmo, ...resto) {
    const nome = typeof algoritmo === "string" ? algoritmo : algoritmo && algoritmo.name;
    if (nome === "Ed25519") {
      entregues += 1;
      return par;
    }
    return original(algoritmo, ...resto);
  };
  window.__impressaoDaBancada = par.then(async ({ publicKey }) => {
    const cru = await sutil.exportKey("raw", publicKey);
    const resumo = new Uint8Array(await sutil.digest("SHA-256", cru));
    return Array.from(resumo, (b) => b.toString(16).padStart(2, "0")).join("");
  });
  window.__chavesEntreguesPelaBancada = () => entregues;
}

/** Instala a identidade fixa em todas as páginas deste contexto. */
async function fixarAIdentidade(contexto) {
  await contexto.addInitScript(instalarNaPagina);
}

/** A impressão (hex64) da pessoa desta página: a que o `--proprietario` lê. */
function impressaoDaPagina(aba) {
  return aba.evaluate(() => window.__impressaoDaBancada);
}

module.exports = { fixarAIdentidade, impressaoDaPagina, instalarNaPagina };
