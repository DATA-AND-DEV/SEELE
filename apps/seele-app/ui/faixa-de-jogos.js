// SEELE — os jogos na barra de baixo da sessão (0.19.2).
//
// # O que este arquivo faz
//
// Um selo na fileira da rede (`#tel-jogos`) por servidor de jogo desta máquina
// (`maquinasNaFaixa`, de `camada-maquina.js`) e por porta aberta para um jogo
// (`tuneisNaFaixa`, de `camada-tuneis.js`): o nome, o estado e o endereço.
//
// Antes eram duas faixas no alto da sessão — a da máquina por cima de tudo, a
// das portas empurrando o conteúdo —, e quem hospedava relatou: «o alerta de
// server ocupa muito espaço útil, deveria ser apenas um badge na faixa de
// baixo». As faixas continuam existindo e continuam sendo a fonte do que se
// diz e dos botões (DESLIGAR, DISPENSAR, COPIAR): dentro da sessão elas saem da
// vista (só da vista — o leitor de tela continua ouvindo o `aria-live`) e
// mudam para dentro do painel quando um selo é clicado.
//
// # Fora da sessão
//
// A barra de baixo é da sessão. Fora dela — a tela de início, com um jogo
// ainda de pé — a faixa de cima volta a dizer tudo, como antes: uma máquina
// rodando um jogo nunca fica sem aviso por falta de onde mostrá-lo.
//
// # Por que do produto
//
// Os selos, o painel e as faixas são do produto, fora de toda superfície de
// MOD: quem diz que uma porta está aberta, ou que um programa roda neste
// computador, é o SEELE, e nenhum MOD alcança este nó.

"use strict";

/** As faixas que o painel recebe, e onde cada uma morava. */
const casasDasFaixasDeJogo = new Map();

/** Se a sessão está à vista: é ela que tem a barra de baixo. */
function jogosNaBarra() {
  const sessao = document.getElementById("tela-sessao");
  return Boolean(sessao && !sessao.hidden);
}

/** O estado e o tom de um servidor de jogo desta máquina. */
function estadoDaMaquinaNaBarra(entrada) {
  const F = FRASES_DA_BARRA_DE_JOGOS;
  if (entrada.sobra) return { tom: "erro", palavra: F.sobrou };
  if (entrada.ligada) {
    if (entrada.motivo === "sem-anfitriao") return { tom: "espera", palavra: F.esperando };
    const rodando = jogosRodando.get(chaveDaMaquina(entrada.alvo, entrada.modId, entrada.servidor)) === true;
    return rodando ? { tom: "rodando", palavra: F.rodando } : { tom: "pronto", palavra: F.pronto };
  }
  // Parada por quem usa, ou por um caminho normal de saída, é cinza; o resto
  // é uma parada que ninguém pediu, e sai vermelha, como na faixa.
  const pedida = new Set(["pelo-dono", "revogada", "app-fechou", "servidor-saiu"]);
  return pedida.has(entrada.motivo) ? { tom: "parado", palavra: F.desligado } : { tom: "erro", palavra: F.parou };
}

/** O estado, o tom e o endereço de uma porta aberta para um jogo. */
function estadoDoTunelNaBarra(entrada) {
  const F = FRASES_DA_BARRA_DE_JOGOS;
  if (entrada.estado === "recusada") return { tom: "erro", palavra: F.recusada, endereco: null };
  if (entrada.estado === "fechada") {
    return { tom: entrada.motivo === "fechado" ? "parado" : "erro", palavra: F.fechada, endereco: null };
  }
  const r = entrada.relatorio;
  const latencia = r && r.latencia_ms !== null && r.latencia_ms !== undefined ? `${r.latencia_ms} ms` : null;
  return {
    tom: "rodando",
    palavra: F.aberta,
    endereco: entrada.porta ? `localhost:${entrada.porta}` : null,
    extra: latencia,
  };
}

/** Um selo: o ponto do tom, o nome, o estado e o endereço. */
function seloDeJogo(chave, nome, estado, frase) {
  const F = FRASES_DA_BARRA_DE_JOGOS;
  const selo = document.createElement("button");
  selo.type = "button";
  selo.className = "selo-de-jogo";
  selo.dataset.tom = estado.tom;
  selo.dataset.chave = chave;
  selo.setAttribute("aria-controls", "painel-jogos");
  selo.setAttribute("aria-expanded", String(painelDeJogosAberto()));
  selo.setAttribute("aria-label", F.detalhes(frase));
  const ponto = document.createElement("span");
  ponto.className = "selo-de-jogo-ponto";
  ponto.setAttribute("aria-hidden", "true");
  const partes = [String(nome).toUpperCase(), estado.palavra, estado.endereco, estado.extra].filter(Boolean);
  const texto = document.createElement("span");
  texto.className = "selo-de-jogo-texto";
  texto.textContent = partes.join(" · ");
  selo.append(ponto, texto);
  selo.addEventListener("click", (evento) => {
    evento.stopPropagation();
    if (painelDeJogosAberto()) fecharOPainelDeJogos();
    else abrirOPainelDeJogos();
  });
  return selo;
}

/** Os selos na barra, e o modo das faixas (na barra ou no alto). */
function desenharOsJogosNaBarra() {
  const lugar = document.getElementById("tel-jogos");
  if (!lugar) return;
  const selos = [];
  if (typeof maquinasNaFaixa !== "undefined") {
    for (const [chave, entrada] of maquinasNaFaixa) {
      const estado = { ...estadoDaMaquinaNaBarra(entrada), endereco: entrada.ligada ? enderecoDaMaquina(entrada) : null };
      const frase = `${entrada.titulo} · ${entrada.servidor} (${entrada.modId}): ${estado.palavra.toLowerCase()}`;
      selos.push(seloDeJogo(`maquina:${chave}`, entrada.servidor, estado, frase));
    }
  }
  if (typeof tuneisNaFaixa !== "undefined") {
    for (const [chave, entrada] of tuneisNaFaixa) {
      const estado = estadoDoTunelNaBarra(entrada);
      const frase = `${entrada.nome} (${entrada.modId}): ${estado.palavra.toLowerCase()}`;
      selos.push(seloDeJogo(`tunel:${chave}`, entrada.nome, estado, frase));
    }
  }
  lugar.replaceChildren(...selos);
  lugar.hidden = selos.length === 0;
  document.documentElement.dataset.jogosNaBarra = jogosNaBarra() ? "sim" : "nao";
  if (selos.length === 0 || !jogosNaBarra()) fecharOPainelDeJogos();
}

function painelDeJogosAberto() {
  const painel = document.getElementById("painel-jogos");
  return Boolean(painel && !painel.hidden);
}

/** Abre o painel acima da barra, com as faixas dentro, como estão. */
function abrirOPainelDeJogos() {
  const painel = document.getElementById("painel-jogos");
  if (!painel || !jogosNaBarra()) return;
  for (const id of ["maquinas", "tuneis"]) {
    const faixa = document.getElementById(id);
    if (!faixa || faixa.parentNode === painel) continue;
    casasDasFaixasDeJogo.set(id, { pai: faixa.parentNode, depois: faixa.nextSibling });
    painel.append(faixa);
  }
  painel.hidden = false;
  posicionarOPainelDeJogos();
  document.documentElement.dataset.jogosAbertos = "sim";
  for (const selo of document.querySelectorAll(".selo-de-jogo")) selo.setAttribute("aria-expanded", "true");
}

/**
 * O pé do painel 8 px acima da barra de baixo, e a borda direita com a dela:
 * medidos da barra de verdade, e não de um número escrito — a altura da barra
 * muda com a quebra de linha das métricas e com o tamanho do texto.
 */
function posicionarOPainelDeJogos() {
  const painel = document.getElementById("painel-jogos");
  const barra = document.querySelector("#tela-sessao footer.telemetria");
  if (!painel || painel.hidden || !barra) return;
  const r = barra.getBoundingClientRect();
  painel.style.bottom = `${Math.max(8, Math.round(innerHeight - r.top + 8))}px`;
  painel.style.right = `${Math.max(8, Math.round(innerWidth - r.right))}px`;
}

window.addEventListener("resize", posicionarOPainelDeJogos);

/** Fecha o painel e devolve cada faixa à casa dela. */
function fecharOPainelDeJogos() {
  const painel = document.getElementById("painel-jogos");
  if (!painel) return;
  for (const [id, casa] of casasDasFaixasDeJogo) {
    const faixa = document.getElementById(id);
    if (faixa && casa.pai && casa.pai.isConnected) {
      casa.pai.insertBefore(faixa, casa.depois && casa.depois.parentNode === casa.pai ? casa.depois : null);
    }
  }
  casasDasFaixasDeJogo.clear();
  painel.hidden = true;
  delete document.documentElement.dataset.jogosAbertos;
  for (const selo of document.querySelectorAll(".selo-de-jogo")) selo.setAttribute("aria-expanded", "false");
}

// O Esc e o clique fora fecham o painel; o clique dentro dele (COPIAR,
// DESLIGAR) não.
document.addEventListener("keydown", (evento) => {
  if (evento.key === "Escape" && painelDeJogosAberto()) fecharOPainelDeJogos();
});
document.addEventListener("click", (evento) => {
  if (!painelDeJogosAberto()) return;
  const painel = document.getElementById("painel-jogos");
  if (painel.contains(evento.target) || evento.target.closest?.(".selo-de-jogo")) return;
  fecharOPainelDeJogos();
});

// A sessão que aparece ou some troca o modo: dentro dela, os selos; fora, a
// faixa de cima.
{
  const sessao = document.getElementById("tela-sessao");
  if (sessao) {
    new MutationObserver(desenharOsJogosNaBarra).observe(sessao, { attributes: true, attributeFilter: ["hidden"] });
  }
  desenharOsJogosNaBarra();
}
