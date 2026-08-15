// SEELE · Entry Plug — a camada comum a toda tela.
//
// Este arquivo desenha e nada mais. `specs/06-clientes-gui.md`: "Nenhuma lógica
// de protocolo em JavaScript. Se o frontend precisa saber o que é um `ssrc`,
// algo está errado." Nada aqui sabe o que é um ssrc, o que faz uma Taxa de
// Sincronização ser crítica, ou quando reconectar. Tudo isso chega decidido
// dentro do snapshot.
//
// O padrão é o mesmo de `seele-tui::view`: projetar o snapshot inteiro a cada
// mudança. Não há estado derivado nem cache — a tela é função de um valor que
// chega pronto. ADR 0019 explica por que isso dispensa framework.
//
// ---- o que mora aqui, e o que a divisão exige ----
//
// A ponte para o Rust, os quatro ajudantes de DOM, os formatadores puros e o
// laço que puxa o snapshot. O que só uma tela usa mora em `tela-<nome>.js`.
//
// Não há módulo nem `import` (ADR 0019): os arquivos dividem o mesmo escopo
// global, e o que muda ao dividir não é a visibilidade, é o **instante**. Uma
// função declarada em `tela-sessao.js` já existe quando qualquer coisa a chama
// de dentro de um manipulador — mas não existe enquanto `tela-boot.js` executa
// seu corpo de topo. Daí a regra que `index.html` segue e
// `apps/seele-app/tests/frontend.rs` confere:
//
//   1. `base.js` primeiro: todo o resto o usa em tempo de execução;
//   2. `glifos.js` e `frases.js` antes das telas;
//   3. cada `tela-<nome>.js` registra os **seus** ouvintes no seu próprio rodapé,
//      porque `addEventListener(…, funcao)` lê a função na hora, e não depois.
//
// `let` no topo de um script clássico é ligação de escopo de script, dividida
// entre todos eles: `tela-fim.js` escreve em `desenhado`, que `tela-sessao.js`
// declara. Isso vale porque só acontece dentro de um manipulador, muito depois
// de os seis arquivos terem rodado.

"use strict";

const { invoke } = window.__TAURI__.core;
const { listen } = window.__TAURI__.event;

const $ = (id) => document.getElementById(id);

// ---------------------------------------------------------------- utilidades

/**
 * O horário local de um instante do servidor.
 *
 * A FFI entrega **segundos** — a unidade está no nome do campo porque errá-la
 * já desenhou toda mensagem como 1970 uma vez.
 */
function relogio(segundos) {
  if (!segundos) return "--:--";
  const quando = new Date(segundos * 1000);
  return quando.toLocaleTimeString([], { hour: "2-digit", minute: "2-digit" });
}

/**
 * Quanto tempo faz, em palavras curtas.
 *
 * A data exata não ajuda a escolher para onde voltar; "ontem" ajuda.
 */
function quando(segundos) {
  if (!segundos) return "—";
  const dias = Math.floor((Date.now() / 1000 - segundos) / 86400);
  if (dias <= 0) return "hoje";
  if (dias === 1) return "ontem";
  return `${dias} dias`;
}

/**
 * A marca de bloco de uma faixa da Taxa de Sincronização.
 *
 * `specs/05-cliente-tui.md`: nenhuma informação transmitida só por cor. A marca
 * é a metade que sobrevive sem cor nenhuma, e é desenhada em toda paleta — uma
 * marca que só aparece quando piora é uma marca que ninguém aprendeu a ler.
 */
function marcaSync(faixa) {
  return { Nominal: "█", Acceptable: "▓", Degraded: "▒", Critical: "░" }[faixa] ?? "░";
}

/** Substitui os filhos de um elemento por uma lista nova. */
function repovoar(pai, filhos) {
  pai.replaceChildren(...filhos);
}

function elemento(tag, classe, texto) {
  const nodo = document.createElement(tag);
  if (classe) nodo.className = classe;
  if (texto !== undefined) nodo.textContent = texto;
  return nodo;
}

// --------------------------------------------------------------- o snapshot

async function atualizar() {
  try {
    desenhar(await invoke("snapshot"));
  } catch (erro) {
    // Sem sessão. Não é uma falha: é o estado antes de conectar e depois de sair.
    if (erro !== "NotConnected") console.warn("snapshot:", erro);
  }
}

function digitando() {
  const ativo = document.activeElement;
  return ativo && (ativo.tagName === "INPUT" || ativo.tagName === "TEXTAREA");
}
