// Quem hospeda vê, em ESTE SERVIDOR, quantas mensagens de apps mais novos o
// servidor pulou — o zero também (a P12 do plano F1b, `396d2980`).
//
// O guarda de `frontend.rs` lê o texto: que a frase diz o zero, que a linha só
// se esconde por `hospedando`. Este roteiro olha a tela. A reversão que ele
// pega é a linha escondida pelo número, que é o zero calado: com ela, quem
// hospeda e nunca viu a linha não sabe se o servidor está limpo ou se a tela
// nem existe.
const NOME = "VERSÕES DE QUEM ENTRA";
const TITULO = "MENSAGENS DE APPS MAIS NOVOS QUE ESTE SERVIDOR";
const REMEDIO = "atualizar o SEELE daqui resolve";
const bloco = () => [...document.querySelectorAll("#painel-servidor .server-bloco")]
  .find((b) => b.querySelector(".server-bloco-nome")?.textContent === NOME);
// As linhas como a tela as mostra, e não o `textContent`: `innerText` segue o
// desenho — uma quebra que o CSS reflui num espaço não vira linha aqui, e o que
// não está à vista não tem texto nenhum.
const linhas = () => [...(bloco()?.querySelectorAll(".nota") ?? [])]
  .flatMap((nota) => nota.innerText.split("\n")).map((l) => l.trim()).filter(Boolean);
const frase = () => linhas().join(" ⏎ ");
const aVista = () => {
  const b = bloco();
  if (!b || b.hidden) return false;
  const caixa = b.getBoundingClientRect();
  return caixa.width > 0 && caixa.height > 0;
};

await entrarNaSessao();
document.getElementById("botao-server-sessao").click();
await espera(300);
document.getElementById("secao-servidor").click();
exigir(await ate(() => bloco() !== undefined, 1500),
  `ESTE SERVIDOR abriu sem o bloco ${NOME}`);

// **Zero, dito.**
exigir(await ate(aVista, 1500),
  "com zero mensagens a linha não está à vista: o zero ficou calado");
exigir(await ate(() => frase() === `${TITULO}: 0`, 1500),
  `com zero, a tela diz «${frase()}»`);
relatar("zero: " + frase());
// Dentro do corpo, como os outros blocos, e não encostada na borda.
const recuo = () => Math.round(bloco().querySelector(".nota").getBoundingClientRect().left -
  bloco().getBoundingClientRect().left);
exigir(recuo() >= 8, `a frase está a ${recuo()} px da borda do bloco`);

// **Três em uma sessão**, com o remédio junto.
window.__SEELE_APPS_MAIS_NOVOS = { mensagens: 3, sessoes: 1 };
const tres = `${TITULO}: 3, EM 1 SESSÃO.`;
exigir(await ate(() => frase().startsWith(tres), 1500),
  `com 3 em 1 sessão, a tela diz «${frase()}»`);
exigir(aVista(), "com 3 mensagens a linha não está à vista");
// O número numa linha e o que fazer na outra: coladas, o remédio vira o fim
// de uma frase comprida sobre o número.
exigir(linhas().length === 2 && linhas()[0] === tres && linhas()[1].includes(REMEDIO),
  `com 3, o número e o remédio não estão cada um na sua linha: «${frase()}»`);
relatar("três: " + frase());
relatar(`a ${recuo()} px da borda`);
