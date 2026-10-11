// A confirmação ao fechar o SEELE com o servidor guardado no ar (tarefa 21 da
// F7; a exceção ao guarda da credencial do lote R da F5, decidida pela
// orquestradora em 10/10).
//
// 1. **Sem servidor no ar, fecha direto**, sem caixa e sem veto.
// 2. **As credenciais da tentativa somem antes de qualquer veto**: a que um
//    apelido tomado deixou (`recusaDoApelido`) é esquecida em todo pedido de
//    fechar.
// 3. **Com o servidor guardado no ar, pergunta**, e o fechar fica segurado até o
//    sim; no sim, a janela fecha, e o pedido seguinte passa direto.
//
// O sistema pede o fechar pelo ouvinte que a casca registra
// (`onCloseRequested`): o duble faz o pedido em `__SEELE_AO_FECHAR()`, e, como
// o Tauri, sem `preventDefault` termina o fechar pelo `destroy()` — com a
// permissão que a capacidade `janela` dá ou não dá. **Fechar é a janela ser
// destruída** (`__SEELE_DESTRUIDA`): sem isso o app fica vivo, dentro do
// servidor, sem despedida, e foi o que a medida de 10/10 achou com este roteiro
// verde.

exigir(typeof window.__SEELE_AO_FECHAR === "function", "a casca não ouve o pedido de fechar a janela");

// 1 e 2. Na entrada, sem servidor no ar, com uma credencial de apelido tomado.
recusaDoApelido = { alvo: "casa.exemplo:8383", credencial: "segredo-de-teste" };
const semServidor = await window.__SEELE_AO_FECHAR();
exigir(!semServidor.segurado, "sem servidor no ar, o fechar foi segurado");
exigir(
  window.__SEELE_DESTRUIDA === true,
  "sem servidor no ar, o pedido de fechar passou e a janela não fechou: o app fica vivo, dentro do servidor",
);
window.__SEELE_DESTRUIDA = false;
exigir(visivel("moderar") === "escondida", "sem servidor no ar, o fechar perguntou alguma coisa");
exigir(recusaDoApelido === null, "o pedido de fechar deixou viva a credencial do apelido tomado");

// 3. Com o servidor guardado no ar.
await entrarNaSessao();
recusaDoApelido = { alvo: "casa.exemplo:8383", credencial: "segredo-de-teste" };
const comServidor = await window.__SEELE_AO_FECHAR();
exigir(comServidor.segurado, "com o servidor no ar, o SEELE fechou sem perguntar");
exigir(!window.__SEELE_DESTRUIDA, "com o servidor no ar, a janela fechou antes da pergunta");
exigir(recusaDoApelido === null, "o fechar segurado manteve viva a credencial do apelido tomado");
await ate(() => visivel("moderar") === "VISIVEL", 1000);
const frase = document.getElementById("moderar-consequencia").textContent;
relatar("a caixa de fechar: " + frase.replace(/\s+/g, " "));
exigir(frase.includes("Casa") && frase.includes("sai do ar"), "a caixa de fechar não diz o que cai: «" + frase + "»");
exigir(!window.__SEELE_FECHAMENTOS, "a janela fechou antes do sim");

document.getElementById("moderar-confirmar").click();
await ate(() => (window.__SEELE_FECHAMENTOS ?? 0) > 0, 1500);
exigir((window.__SEELE_FECHAMENTOS ?? 0) > 0, "o sim da caixa não pediu o fechar da janela");
await ate(() => window.__SEELE_DESTRUIDA === true, 1500);
exigir(window.__SEELE_DESTRUIDA === true, "o sim da caixa pediu o fechar e a janela não fechou");
const depoisDoSim = await window.__SEELE_AO_FECHAR();
exigir(!depoisDoSim.segurado, "depois do sim, o pedido de fechar foi segurado de novo");
