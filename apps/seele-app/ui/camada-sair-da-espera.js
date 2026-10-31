// SEELE — sair da sala de espera (`#sair-da-espera`, 0.19).
//
// **Quem espera a aprovação de quem hospeda não está num erro**, e saía achando
// que estava: «um aviso quando uma pessoa tentar sair da tela de espera quando
// aguarda aprovação, para ela não pensar que deu errado», pediu o dono. A
// portaria (ADR 0030) derruba a conexão no instante e guarda o pedido no banco
// do servidor; esperar é o caminho normal de toda pessoa nova.
//
// Esta caixa aparece quando a pessoa tenta sair com o pedido ainda pendente, e
// diz três coisas, nesta ordem: que nada deu errado; o que sair faz com o pedido
// (não o cancela — ele fica no servidor, e `portaria::pedido_pendente` perdoa a
// volta sem convite); e como voltar. O padrão é CONTINUAR ESPERANDO: o foco
// nasce nele, e Esc, apertar fora e o voltar do aparelho também ficam.
//
// **Por onde se sai**, e cada caminho chama `perguntarAntesDeSairDaEspera`:
//
// - `#tela-auth` em espera (o desktop, a casca larga, e o celular quando a
//   entrada passa por ela): o VOLTAR À ENTRADA e o Esc (`tela-auth.js`);
// - a «ENTRAR EM» do celular, depois de uma batida que ficou na portaria: o
//   AGORA NÃO e o ‹ VOLTAR (`celular-entrada.js`).
//
// **Fechar a janela não pergunta**, e é decisão, não esquecimento. No app, o
// fechar só é segurado com o servidor guardado no ar (tarefa 21 da F7; spec de
// 23/09: «a confirmação ao fechar vale só para o servidor guardado»), e o guarda
// `a_credencial_da_tentativa_so_vai_ao_connect_e_morre_com_a_pergunta` prende
// isso. No navegador, o `beforeunload` só mostra a frase genérica dele («sair
// do site?»), que diria que algo se perde — e nada se perde: o pedido continua
// no servidor.

"use strict";

/** O que SAIR MESMO ASSIM faz, enquanto a caixa está aberta; senão `null`. */
let saidaDaEspera = null;

/** Quem tinha o teclado, para CONTINUAR ESPERANDO o devolver. */
let focoAntesDeSairDaEspera = null;

/** Tira a caixa da pilha do voltar do celular, quando ela entrou nela. */
let soltarDoVoltar = null;

/**
 * Pergunta antes de sair de uma espera pendente.
 *
 * `modo`: `"sair"` (a `#tela-auth`, que confere sozinha) ou `"aparelho"` (a
 * «ENTRAR EM» do celular, onde o servidor fica na lista do aparelho). `sair` roda no SAIR
 * MESMO ASSIM, e só nele.
 */
function perguntarAntesDeSairDaEspera(modo, sair) {
  const f = FRASES_DA_ESPERA;
  // No navegador a volta é pela lista do aparelho: o endereço sozinho não abre
  // conexão lá (`entrarPeloEndereco`, em `ponte-web.js`). Quem diz é a casca.
  const doAparelho = modo === "aparelho" || (modo === "sair" && CASCA.esperaNoAparelho === true);
  const itens = [
    f.nadaErrado,
    doAparelho ? f.sairCelular : f.sair,
    doAparelho ? f.voltarCelular : f.voltar,
  ];
  focoAntesDeSairDaEspera = document.activeElement;
  saidaDaEspera = sair;
  $("sair-da-espera-rotulo").textContent = f.rotulo;
  $("sair-da-espera-titulo").textContent = f.titulo;
  $("sair-da-espera-itens").replaceChildren(...itens.map((texto) => elemento("li", null, texto)));
  $("sair-da-espera-sair").textContent = f.sairMesmo;
  $("sair-da-espera-ficar").textContent = f.ficar;
  $("sair-da-espera").dataset.modo = modo;
  $("sair-da-espera").hidden = false;
  $("sair-da-espera-ficar").focus();
  anunciar(`${f.titulo}. ${f.nadaErrado}`);
  // O voltar do celular fecha a camada de cima: aqui, continuar esperando.
  const empilhar = typeof CELULAR === "object" && CELULAR ? CELULAR.ajudante("empilhar") : null;
  soltarDoVoltar?.();
  soltarDoVoltar = typeof empilhar === "function" ? empilhar(() => fecharSairDaEspera(false)) : null;
}

/**
 * Fecha a caixa. `devolverFoco` é falso quando ela fecha sozinha (a espera
 * acabou por baixo dela) ou pelo voltar do aparelho, que já leva o foco.
 */
function fecharSairDaEspera(devolverFoco = true) {
  if ($("sair-da-espera").hidden) return;
  $("sair-da-espera").hidden = true;
  saidaDaEspera = null;
  const soltar = soltarDoVoltar;
  soltarDoVoltar = null;
  soltar?.();
  if (devolverFoco && focavel(focoAntesDeSairDaEspera)) focoAntesDeSairDaEspera.focus();
  focoAntesDeSairDaEspera = null;
}

/** CONTINUAR ESPERANDO: nada muda, e a espera segue de onde estava. */
function continuarEsperando() {
  fecharSairDaEspera(true);
}

$("sair-da-espera-ficar").addEventListener("click", continuarEsperando);

$("sair-da-espera-sair").addEventListener("click", () => {
  const sair = saidaDaEspera;
  fecharSairDaEspera(false);
  if (typeof sair !== "function") return;
  Promise.resolve()
    .then(sair)
    .catch((falha) => console.warn("sair da espera:", falha));
});

fecharAoClicarFora("sair-da-espera", continuarEsperando);

// Esc é CONTINUAR ESPERANDO, em captura: com a caixa aberta, ela é a de cima.
window.addEventListener(
  "keydown",
  (evento) => {
    if (evento.key !== "Escape" || $("sair-da-espera").hidden) return;
    evento.preventDefault();
    evento.stopPropagation();
    continuarEsperando();
  },
  true,
);

// **A espera pode acabar por baixo da caixa**: a batida seguinte entra (quem
// hospeda admitiu) ou volta recusada. Aí a pergunta mente — não há mais pedido
// esperando —, e ela fecha sozinha. Só no modo da `#tela-auth`: a «ENTRAR EM»
// do celular não bate sozinha, e nada acaba por baixo dela.
new MutationObserver(() => {
  const modo = $("sair-da-espera").dataset.modo;
  if ($("sair-da-espera").hidden || modo === "aparelho") return;
  if (!esperaPendenteNaTela()) fecharSairDaEspera(false);
}).observe($("tela-auth"), { attributes: true, attributeFilter: ["hidden", "data-modo", "data-espera"] });
