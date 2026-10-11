// A caixa de compartilhar pergunta o que mostrar, a resolução, os quadros e se
// inclui o som — e mais nada.
//
// **O que ela pergunta mudou duas vezes, e este roteiro ficou na primeira.**
// Em `1e485bfc` (01/09/2026) a caixa passou a perguntar «uma coisa só»: a
// prioridade, a altura, os quadros e a banda saíram. Em `c048f752` (21/09) os
// quadros voltaram, de propósito, como escolha de teto com 30 e 60 — `Q8` e
// `Q15` ficam como piso automático e não se oferecem. E o INCLUIR SOM entrou
// pelo R21 da revisão da v15. Este roteiro continuou procurando os quadros
// entre os que tinham saído, relatou «compartilhar-quadros: AINDA EXISTE» de
// 21/09 a 10/10/2026, e ficou verde, sem nenhum `exigir` (lote RT da F5).
await entrarNaSessao();
await abrirChamada();
await espera(300);
// O botão mora no rodapé do operador desde que a navegação passou para as
// listas; ver `desenharOperador`.
document.getElementById("operador-vista").click();
await espera(500);
relatar("caixa: " + visivel("compartilhar"));
exigir(visivel("compartilhar") === "VISIVEL",
  "o botão do rodapé não abriu a caixa de compartilhar: " + visivel("compartilhar"));

// O que a caixa pergunta: os controles de escolha dentro dela. A lista de
// fontes são botões, um por monitor ou janela, e não entra aqui.
const caixa = document.getElementById("compartilhar");
const perguntas = [...caixa.querySelectorAll("select, input, textarea")]
  .map((el) => el.id || el.tagName.toLowerCase()).sort();
relatar("o que ela pergunta: " + perguntas.join(" "));
exigir(perguntas.join(" ") === "compartilhar-quadros compartilhar-resolucao compartilhar-som",
  "a caixa pergunta outra coisa além da resolução, dos quadros e do som: " + perguntas.join(" "));

// Os que saíram em `1e485bfc` continuam fora, em qualquer forma.
const voltaram = ["compartilhar-prioridade", "compartilhar-altura", "compartilhar-banda"]
  .filter((id) => document.getElementById(id));
relatar("os que saíram e voltaram: " + (voltaram.join(" ") || "nenhum"));
exigir(voltaram.length === 0, "voltou à caixa o que tinha saído: " + voltaram.join(" "));

// Os quadros oferecem 30 e 60, e só: o piso automático não é escolha.
const quadros = [...document.getElementById("compartilhar-quadros").options].map((o) => o.value);
relatar("quadros oferecidos: " + quadros.join(" "));
exigir(quadros.join(" ") === "30 60",
  "os quadros oferecem outra coisa que 30 e 60: " + quadros.join(" "));
