// SEELE — a máquina de jogo, na casca (API 8, parte 3).
//
// # O que este arquivo desenha
//
// Três coisas **do produto**, fora de toda superfície de MOD:
//
//  1. a **tela do aceite** (`#maquina-aceite`): quando o servidor pede que este
//     computador rode o servidor de um jogo, o SEELE mostra o que vai baixar, o
//     que vai rodar, quanto de disco ocupa, e os termos do jogo, e só liga se a
//     pessoa disser que sim;
//  2. a **faixa das máquinas ligadas** (`#maquinas`, global: em toda tela): uma
//     linha por máquina de pé, com DESLIGAR, e a última parada com o motivo em frase;
//  3. a **lista de aceites** nas configurações, com DESLIGAR (se ligada) e REVOGAR.
//
// # Por que fora das superfícies de MOD
//
// O MOD **oferece** a máquina; quem pergunta se ela roda é o produto. Uma tela
// que um MOD pudesse desenhar, calar ou cobrir seria um programa rodando na
// máquina de alguém que nunca viu o que ele faz. Nada daqui passa por
// `mods-regiao.js`, e o texto de tudo o que o MOD escolheu (título, origens,
// programa, termos) entra por `textContent`, nunca como marcação.
//
// # O que a tela garante
//
// - O aviso de que o servidor roda **com a conta de quem aceita** fica no alto,
//   fora da área que rola: está à vista sem rolar, em qualquer tamanho de janela.
// - ACEITAR fica desligado até todo termo ter a caixa marcada.
// - Ela não fecha por fora. Esc é RECUSAR, e a pessoa vê a tela fechar: é o lado
//   seguro, e um clique perdido no véu não pode ser um «não» nem um «sim».
// - Um pedido por vez. Os que chegam enquanto um está na tela esperam em fila.
//
// **Nada aqui engole uma falha**: o que o Rust recusa aparece na tela, em frase,
// e o que a lista de frases não conhece vai ao `seele.log` com o nome cru.

"use strict";

/** Os pedidos que esperam a vez, na ordem em que chegaram. */
const filaDeMaquinas = [];

/** O pedido que está na tela: `{ pedido, caixas, enviando, morto }`, ou `null`. */
let maquinaNaTela = null;

/** Onde estava o foco antes da tela, para devolvê-lo a quem abriu a conversa. */
let focoAntesDaMaquina = null;

/** As linhas da faixa, por chave: ligadas e a última parada de cada uma. */
const maquinasNaFaixa = new Map();

/** O título que cada máquina teve no último pedido, para a faixa e as paradas. */
const titulosDeMaquinas = new Map();

/**
 * Se o jogo de cada máquina ligada roda agora (`maquina-jogo`, e `jogo` de
 * `maquinas_aceitas`): a faixa diz «serve» só com o jogo de pé.
 */
const jogosRodando = new Map();

/**
 * O que, vindo de `maquina-estado`, também desfaz um pedido que ainda espera
 * resposta: o pedido deixou de existir, e aceitá-lo já não leva a nada. O Rust
 * só diz isso por uma chave que tinha pedido pendente ou máquina de pé
 * (`dizer_pedidos_desfeitos`, `encerrar_e_dizer`). Os outros motivos falam de
 * uma máquina que estava de pé.
 */
const MOTIVOS_QUE_DISPENSAM_O_PEDIDO = new Set(["mod-desligado", "outro-pedido", "sem-aceite", "servidor-saiu"]);

/** Erros de aceite/recusa que dizem que o pedido já não está lá para ser respondido. */
const ERROS_DE_PEDIDO_QUE_SUMIU = new Set([
  "sem-pedido", "pedido-velho", "sem-conexao", "sem-sessao", "sessao-encerrada",
]);

function chaveDaMaquina(alvo, modId, servidor) {
  return `${alvo}\u0000${modId}\u0000${servidor}`;
}

function chaveDoEvento(e) {
  return chaveDaMaquina(e.alvo, e.mod_id, e.servidor);
}

/** O código de uma falha de comando, como a casca o recebe (texto, ou `Recusado`). */
function codigoDaFalhaDaMaquina(falha) {
  if (typeof falha === "string") return falha;
  if (falha?.Recusado?.motivo) return String(falha.Recusado.motivo);
  return String(falha?.message ?? falha);
}

/** A frase de uma falha, ou o código cru com a promessa de que o registro o tem. */
function fraseDaFalhaDaMaquina(falha) {
  const codigo = codigoDaFalhaDaMaquina(falha);
  const frase = FRASES_DA_MAQUINA.erros[codigo];
  if (typeof frase === "string") return FRASES_DA_MAQUINA.falhou(frase);
  registrarNoAnfitriao("maquina", `a tela não tem frase para a falha «${codigo}»`, "aviso");
  return FRASES_DA_MAQUINA.falhouSemFrase(codigo);
}

/** A frase de um nome da lista, ou o próprio nome quando a lista não o conhece (e isso é dito). */
function fraseDaListaDaMaquina(lista, nome) {
  const frase = FRASES_DA_MAQUINA[lista]?.[nome];
  if (typeof frase === "string") return frase;
  registrarNoAnfitriao("maquina", `a tela não tem frase para «${nome}» (${lista})`, "aviso");
  return String(nome);
}

/** Os campos que mudaram, em frase e separados por vírgula: cada nome tem a sua frase na lista. */
function camposQueMudaram(mudou) {
  return mudou.map((campo) => fraseDaListaDaMaquina("campos", campo)).join(", ");
}

/** De onde vem um arquivo: o host da URL dele, ou a URL inteira se ela não se lê. */
function origemDoArtefato(artefato) {
  try {
    return new URL(artefato.url).host;
  } catch {
    return String(artefato.url);
  }
}

/** As linhas de fato do pedido, na ordem em que a pessoa as lê. */
function linhasDoPedido(pedido) {
  const oferta = pedido.oferta;
  const F = FRASES_DA_MAQUINA;
  const linhas = [
    F.mod(pedido.mod_nome, pedido.mod_id),
    F.jogo(oferta.titulo, oferta.versao_do_jogo),
  ];
  if (oferta.receita) linhas.push(F.receita(oferta.receita, oferta.versao_da_receita));
  linhas.push(F.pedidoPor(pedido.pedido_por, pedido.servidor_seele || pedido.alvo));
  for (const tunel of oferta.tuneis ?? []) linhas.push(F.porta(String(tunel.nome ?? ""), tunel.porta, tunel.protocolo));

  // **Só o que este computador baixa** (API 8 parte 4): o Rust separa, pela
  // mesma regra da política do `programa`, os artefatos deste sistema dos de
  // outros (`artefatos_daqui`, `de_outros_sistemas`). Somar os da oferta
  // inteira era dizer cinco Javas quando se baixa um.
  const artefatos = pedido.artefatos_daqui ?? [];
  const deOutros = Number(pedido.de_outros_sistemas) || 0;
  if (artefatos.length === 0) {
    linhas.push(F.nadaABaixar);
  } else {
    const total = artefatos.reduce((soma, a) => soma + (Number(a.tamanho) || 0), 0);
    linhas.push(F.baixar(artefatos.length, emBytes(total)));
    for (const a of artefatos) linhas.push(F.artefato(a.nome, origemDoArtefato(a), emBytes(Number(a.tamanho) || 0)));
  }
  if (deOutros > 0) linhas.push(F.deOutrosSistemas(deOutros));
  if ((oferta.origens ?? []).length > 0) linhas.push(F.podeBaixarDe(oferta.origens.join(", ")));
  // **O pack, pelo hash** (API 8 parte 6): é o que a máquina confere quando o
  // traz da metade de servidor; o começo do hash basta para comparar a olho.
  if (oferta.pack && oferta.pack.hex) {
    linhas.push(F.pack(String(oferta.pack.algoritmo), String(oferta.pack.hex).slice(0, 16)));
  }
  // **Os dois comandos** (API 8 parte 6): o instalador do loader e o servidor.
  // **Os deste computador** (decisão W1): o Rust escolhe, pela regra da
  // política, o par da entrada de `por_sistema` deste sistema ou o de cima
  // (`rodar_daqui`, `instalar_daqui`). Mostrar o `rodar` cru da oferta era
  // dizer `unix_args.txt` no Windows, que roda o `win_args.txt`.
  const instalar = "instalar_daqui" in pedido ? pedido.instalar_daqui : oferta.instalar;
  const rodar = "rodar_daqui" in pedido ? pedido.rodar_daqui : oferta.rodar;
  if (instalar) {
    linhas.push(F.instalar([instalar.comando, ...(instalar.argumentos ?? [])].join(" ")));
  }
  if (rodar) {
    linhas.push(F.rodar([rodar.comando, ...(rodar.argumentos ?? [])].join(" ")));
  } else {
    linhas.push(F.nadaARodar);
  }
  const outrosSistemas = Number(pedido.sistemas_com_outros_comandos) || 0;
  if (outrosSistemas > 0) linhas.push(F.comandosDeOutrosSistemas(outrosSistemas));
  if ((oferta.dominios_do_jogo ?? []).length > 0) {
    linhas.push(F.oJogoBaixaSozinho(oferta.dominios_do_jogo.join(", ")));
  }
  linhas.push(F.disco(emBytes(Number(oferta.disco_maximo) || 0), pedido.pasta));
  return linhas;
}

/** ACEITAR só liga quando toda caixa de termo está marcada (e nada está em voo). */
function atualizarOAceitar() {
  const atual = maquinaNaTela;
  if (!atual) return;
  const todos = atual.caixas.every((c) => c.checked);
  $("maquina-aceitar").disabled = !(todos && !atual.enviando && !atual.morto);
}

function mostrarOErroDaMaquina(frase) {
  const erro = $("maquina-erro");
  // Revelado antes de escrito, como todo `role="alert"` desta janela.
  erro.hidden = false;
  erro.textContent = frase;
}

/** Desenha o pedido na tela e a abre. */
function desenharOPedido(pedido) {
  const F = FRASES_DA_MAQUINA;
  // A linha miúda de cima (0.19): o MOD e o servidor, e as portas num chip —
  // dados do pedido, sem frase nova.
  $("maquina-rotulo").textContent = `${pedido.mod_nome || pedido.mod_id} · ${pedido.servidor}`;
  $("maquina-portas").textContent = (pedido.oferta.tuneis ?? [])
    .map((tunel) => `${tunel.porta}/${String(tunel.protocolo).toUpperCase()}`)
    .join(" · ");
  $("maquina-titulo").textContent = F.titulo(pedido.pedido_por);
  $("maquina-sua-conta").textContent = F.suaConta;

  const mudou = $("maquina-mudou");
  if (Array.isArray(pedido.mudou)) {
    mudou.hidden = false;
    mudou.textContent = F.mudou(camposQueMudaram(pedido.mudou));
  } else {
    mudou.hidden = true;
    mudou.textContent = "";
  }

  repovoar($("maquina-detalhes"), linhasDoPedido(pedido).map((texto) => elemento("li", "maquina-detalhe", texto)));

  const caixas = [];
  const termos = pedido.oferta.termos ?? [];
  repovoar($("maquina-termos"), termos.map((termo) => {
    const item = elemento("li", "maquina-termo");
    const rotulo = elemento("label", "maquina-termo-rotulo");
    const caixa = document.createElement("input");
    caixa.type = "checkbox";
    caixa.className = "maquina-termo-caixa";
    caixa.addEventListener("change", atualizarOAceitar);
    caixas.push(caixa);
    rotulo.append(caixa, elemento("span", "maquina-termo-texto", F.termo(termo.titulo)));
    item.append(rotulo);
    // O endereço aparece escrito: a pessoa vê para onde LER leva antes de clicar.
    const ler = elemento("button", "maquina-ler", F.ler);
    ler.type = "button";
    ler.addEventListener("click", () => {
      invoke("abrir_no_navegador", { url: termo.url }).catch((falha) => {
        mostrarOErroDaMaquina(fraseDaFalhaDaMaquina(falha));
      });
    });
    item.append(elemento("span", "maquina-termo-url", termo.url), ler);
    return item;
  }));

  // «Quem aceita os termos é você» só faz sentido se há termos a aceitar.
  const garantias = [];
  if (termos.length > 0) garantias.push(F.quemAceitaEVoce);
  garantias.push(F.podeDesligar);
  repovoar($("maquina-garantias"), garantias.map((texto) => elemento("li", "maquina-garantia", texto)));

  $("maquina-erro").hidden = true;
  $("maquina-erro").textContent = "";
  $("maquina-recusar").textContent = F.recusar;
  $("maquina-aceitar").textContent = F.aceitar;
  $("maquina-recusar").disabled = false;
  maquinaNaTela = { pedido, caixas, enviando: false, morto: false };
  atualizarOAceitar();
  $("maquina-corpo").scrollTop = 0;
}

function abrirOPedido(pedido) {
  if ($("maquina-aceite").hidden) {
    focoAntesDaMaquina = document.activeElement;
    desenharOPedido(pedido);
    $("maquina-aceite").hidden = false;
  } else {
    desenharOPedido(pedido);
  }
  // O foco nasce na saída que não muda nada.
  $("maquina-recusar").focus();
}

function fecharATela() {
  $("maquina-aceite").hidden = true;
  maquinaNaTela = null;
  const volta = focoAntesDaMaquina;
  focoAntesDaMaquina = null;
  if (volta && document.contains(volta) && !volta.disabled && typeof volta.focus === "function") volta.focus();
}

/** Mostra o próximo da fila, ou fecha. */
function passarAoProximoPedido() {
  const proximo = filaDeMaquinas.shift();
  if (proximo) abrirOPedido(proximo);
  else fecharATela();
}

/** Um pedido chegou: na tela se ela está livre, na fila se não. */
function chegouUmPedido(pedido) {
  const chave = chaveDoEvento(pedido);
  titulosDeMaquinas.set(chave, pedido.oferta?.titulo ?? pedido.mod_id);
  const atual = maquinaNaTela?.pedido;
  if (atual && chaveDoEvento(atual) === chave) {
    // O mesmo pedido outra vez (o servidor reenvia) não mexe nas caixas marcadas;
    // um pedido diferente da mesma máquina vale no lugar do que a pessoa lê.
    if (atual.resumo === pedido.resumo && JSON.stringify(atual.mudou) === JSON.stringify(pedido.mudou)) return;
    abrirOPedido(pedido);
    return;
  }
  const naFila = filaDeMaquinas.findIndex((p) => chaveDoEvento(p) === chave);
  if (naFila >= 0) {
    filaDeMaquinas[naFila] = pedido;
    return;
  }
  if (atual) filaDeMaquinas.push(pedido);
  else abrirOPedido(pedido);
}

/** ACEITAR ou RECUSAR: manda a resposta ao Rust e só então passa ao próximo. */
async function responderAMaquina(aceitou) {
  const atual = maquinaNaTela;
  if (!atual || atual.enviando) return;
  if (aceitou && !atual.caixas.every((c) => c.checked)) return;
  const { pedido } = atual;
  const args = { alvo: pedido.alvo, modId: pedido.mod_id, servidor: pedido.servidor };
  atual.enviando = true;
  $("maquina-erro").hidden = true;
  $("maquina-recusar").disabled = true;
  atualizarOAceitar();
  try {
    if (aceitou) await invoke("maquina_aceitar", { ...args, resumo: pedido.resumo });
    else await invoke("maquina_recusar", args);
  } catch (falha) {
    if (maquinaNaTela !== atual) return;
    atual.enviando = false;
    $("maquina-recusar").disabled = false;
    const codigo = codigoDaFalhaDaMaquina(falha);
    // Recusar um pedido que já não existe não tem o que recusar: a tela fecha.
    if (!aceitou && ERROS_DE_PEDIDO_QUE_SUMIU.has(codigo)) {
      passarAoProximoPedido();
      return;
    }
    // Aceitar um pedido que sumiu não leva a nada, e a tela diz isso em vez de ficar muda.
    if (aceitou && ERROS_DE_PEDIDO_QUE_SUMIU.has(codigo)) atual.morto = true;
    mostrarOErroDaMaquina(fraseDaFalhaDaMaquina(falha));
    atualizarOAceitar();
    $("maquina-recusar").focus();
    return;
  }
  // Se um `maquina-estado` dispensou este pedido enquanto o comando andava, a
  // tela já passou ao próximo e não há o que fechar.
  if (maquinaNaTela !== atual) return;
  passarAoProximoPedido();
}

$("maquina-aceitar").addEventListener("click", () => {
  responderAMaquina(true).catch((falha) => registrarNoAnfitriao("maquina", `aceitar falhou — ${String(falha?.message ?? falha)}`, "aviso"));
});
$("maquina-recusar").addEventListener("click", () => {
  responderAMaquina(false).catch((falha) => registrarNoAnfitriao("maquina", `recusar falhou — ${String(falha?.message ?? falha)}`, "aviso"));
});

// Esc é RECUSAR, em qualquer ponto da janela enquanto a tela está aberta, e
// nenhuma camada de baixo vê a tecla. O Tab não sai da caixa (`aria-modal`).
document.addEventListener("keydown", (evento) => {
  if ($("maquina-aceite").hidden) return;
  if (evento.key === "Escape") {
    evento.preventDefault();
    evento.stopPropagation();
    responderAMaquina(false).catch((falha) => registrarNoAnfitriao("maquina", `recusar falhou — ${String(falha?.message ?? falha)}`, "aviso"));
    return;
  }
  if (evento.key === "Tab") {
    const alcancaveis = [...$("maquina-aceite").querySelectorAll("button:not(:disabled), input:not(:disabled)")];
    if (alcancaveis.length === 0) return;
    const [primeiro, ultimo] = [alcancaveis[0], alcancaveis.at(-1)];
    const dentro = $("maquina-aceite").contains(document.activeElement);
    if (!dentro || (evento.shiftKey && document.activeElement === primeiro)) {
      evento.preventDefault();
      (evento.shiftKey ? ultimo : primeiro).focus();
    } else if (!evento.shiftKey && document.activeElement === ultimo) {
      evento.preventDefault();
      primeiro.focus();
    }
  }
}, true);

// Clicar no véu não é resposta: o foco volta à caixa, e nada é enviado.
$("maquina-aceite").addEventListener("mousedown", (evento) => {
  if (evento.target === $("maquina-aceite")) {
    evento.preventDefault();
    $("maquina-recusar").focus();
  }
});

// ---------------------------------------------------------- a faixa de máquinas

function tituloDaMaquina(chave, modId) {
  return titulosDeMaquinas.get(chave) ?? modId;
}

function desenharUmaMaquina(entrada) {
  const F = FRASES_DA_MAQUINA;
  const bloco = elemento("div", "maquina-bloco");
  bloco.dataset.estado = entrada.ligada ? "ligada" : "parada";
  bloco.dataset.mod = entrada.modId;
  // O motivo da parada pinta a faixa: a que ninguém pediu sai vermelha. De pé,
  // o único motivo é seguir sem quem hospeda (`sem-anfitriao`): laranja.
  if (entrada.motivo) bloco.dataset.motivo = entrada.motivo;
  const linha = elemento("p", "maquina-linha");
  if (entrada.sobra) {
    // O jogo que sobrou de um app que fechou sem desligá-lo: o que se achou e o
    // que se fez, numa frase só.
    const s = entrada.sobra;
    const fim = F.sobras[s.desfecho];
    const desfecho = typeof fim === "function"
      ? fim(s.desfecho === "nao-parou" ? String(s.motivo ?? "") : (Number(s.segundos) || 0).toFixed(1).replace(".", ","))
      : fraseDaListaDaMaquina("sobras", s.desfecho);
    linha.append(elemento("span", "maquina-marca", F.sobra(entrada.titulo, entrada.modId, s.pid)), `: ${desfecho}.`);
  } else if (entrada.ligada) {
    const rodando = jogosRodando.get(chaveDaMaquina(entrada.alvo, entrada.modId, entrada.servidor)) === true;
    bloco.dataset.jogo = rodando ? "rodando" : "parado";
    linha.append(elemento("span", "maquina-marca", (rodando ? F.faixa : F.faixaSemJogo)(entrada.titulo, entrada.modId)));
  } else {
    linha.append(
      elemento("span", "maquina-marca", `${entrada.titulo} (${entrada.modId})`),
      `: ${fraseDaListaDaMaquina("paradas", entrada.motivo)}`,
    );
  }
  bloco.append(linha);
  // **Sem quem hospeda, a máquina segue rodando** e diz até quando espera.
  if (entrada.ligada && entrada.motivo === "sem-anfitriao") bloco.append(elemento("p", "maquina-sem-anfitriao", F.semAnfitriao));
  if (entrada.falha) bloco.append(elemento("p", "maquina-nota", entrada.falha));

  const botao = elemento("button", entrada.ligada ? "maquina-desligar" : "maquina-dispensar", entrada.ligada ? F.desligar : F.dispensar);
  botao.type = "button";
  botao.addEventListener("click", () => {
    if (!entrada.ligada) {
      for (const [chave, outra] of maquinasNaFaixa) if (outra === entrada) maquinasNaFaixa.delete(chave);
      desenharAFaixaDeMaquinas();
      return;
    }
    desligarUmaMaquina(entrada, botao).catch((falha) => {
      registrarNoAnfitriao("maquina", `desligar falhou — ${String(falha?.message ?? falha)}`, "aviso", entrada.modId);
    });
  });
  bloco.append(botao);
  return bloco;
}

async function desligarUmaMaquina(entrada, botao) {
  botao.disabled = true;
  entrada.falha = null;
  try {
    await invoke("maquina_desligar", { alvo: entrada.alvo, modId: entrada.modId, servidor: entrada.servidor });
  } catch (falha) {
    // A máquina segue de pé, e a faixa diz por que o clique não a desligou.
    entrada.falha = fraseDaFalhaDaMaquina(falha);
    desenharAFaixaDeMaquinas();
  }
  // Se deu certo, quem muda a linha é `maquina-estado`: a faixa só diz o que o Rust confirmou.
}

function desenharAFaixaDeMaquinas() {
  const faixa = document.getElementById("maquinas");
  if (!faixa) return;
  faixa.replaceChildren(...[...maquinasNaFaixa.values()].map(desenharUmaMaquina));
  faixa.hidden = maquinasNaFaixa.size === 0;
}

function aplicarOEstadoDaMaquina(estado) {
  const chave = chaveDoEvento(estado);
  const entrada = {
    alvo: estado.alvo, modId: estado.mod_id, servidor: estado.servidor,
    titulo: tituloDaMaquina(chave, estado.mod_id),
    ligada: estado.ligada === true,
    motivo: estado.motivo ?? null,
    falha: null,
  };
  // A metade que parou levou o jogo junto (`encerrar_tudo`).
  if (!entrada.ligada) jogosRodando.delete(chave);
  maquinasNaFaixa.delete(chave);
  maquinasNaFaixa.set(chave, entrada);
  desenharAFaixaDeMaquinas();
  // A lista das configurações, se está à vista, acompanha o que o Rust confirmou.
  if ($("lista-maquinas-aceitas").offsetParent !== null) {
    desenharMaquinasAceitas().catch((falha) => {
      registrarNoAnfitriao("maquina", `a lista de aceites não redesenhou — ${String(falha?.message ?? falha)}`, "aviso");
    });
  }

  if (!entrada.ligada && MOTIVOS_QUE_DISPENSAM_O_PEDIDO.has(entrada.motivo)) {
    // **Só o pedido que o aviso nomeia** (`resumo`): um aviso sem ele é o de uma
    // máquina que estava de pé, e um pedido NOVO da mesma chave, na tela ou na
    // fila, não é o que acabou. O pedido que acabou não existe mais: aceitá-lo
    // daria `sem-pedido`. Sai da fila e da tela, e a faixa acima diz por quê.
    if (typeof estado.resumo === "string") {
      for (let i = filaDeMaquinas.length - 1; i >= 0; i--) {
        if (chaveDoEvento(filaDeMaquinas[i]) === chave && filaDeMaquinas[i].resumo === estado.resumo) {
          filaDeMaquinas.splice(i, 1);
        }
      }
      if (maquinaNaTela && chaveDoEvento(maquinaNaTela.pedido) === chave
          && maquinaNaTela.pedido.resumo === estado.resumo) {
        passarAoProximoPedido();
      }
    }
  }
  if (!entrada.ligada && entrada.motivo === "pacote-diferente" && maquinaNaTela
      && chaveDoEvento(maquinaNaTela.pedido) === chave && !maquinaNaTela.enviando) {
    passarAoProximoPedido();
  }
}

// ------------------------------------------------------------- a lista de aceites

/** A última falha de `maquinas_aceitas`, para a lista dizê-la (M9). */
let falhaDasMaquinasAceitas = null;

/** Lê o que o Rust guarda (`maquinas_aceitas`), ou `null` dizendo ao registro o que falhou. */
async function lerAsMaquinasAceitas() {
  try {
    const aceitas = await invoke("maquinas_aceitas");
    falhaDasMaquinasAceitas = null;
    return Array.isArray(aceitas) ? aceitas : [];
  } catch (falha) {
    falhaDasMaquinasAceitas = falha;
    registrarNoAnfitriao("maquina", `maquinas_aceitas falhou — ${codigoDaFalhaDaMaquina(falha)}`, "aviso");
    return null;
  }
}

/** A faixa depois de recarregar a janela: as máquinas que já estavam de pé. */
async function relerAsMaquinasLigadas() {
  const aceitas = await lerAsMaquinasAceitas();
  for (const aceita of aceitas ?? []) {
    const chave = chaveDaMaquina(aceita.alvo, aceita.mod_id, aceita.servidor);
    titulosDeMaquinas.set(chave, aceita.titulo);
    if (aceita.ligada && typeof aceita.jogo === "boolean" && !jogosRodando.has(chave)) jogosRodando.set(chave, aceita.jogo);
    if (!aceita.ligada || maquinasNaFaixa.has(chave)) continue;
    maquinasNaFaixa.set(chave, {
      alvo: aceita.alvo, modId: aceita.mod_id, servidor: aceita.servidor,
      titulo: aceita.titulo, ligada: true, motivo: null, falha: null,
    });
  }
  desenharAFaixaDeMaquinas();
}

/** A lista das configurações: o que esta pessoa aceitou rodar, com REVOGAR. */
async function desenharMaquinasAceitas() {
  const F = FRASES_DA_MAQUINA;
  const lista = $("lista-maquinas-aceitas");
  const aceitas = await lerAsMaquinasAceitas();
  if (aceitas === null) {
    // **A falha na tela, e não só no registro** (revisão final, M9): uma lista
    // vazia sem frase deixava um jogo de pé sem DESLIGAR, calado.
    const erro = $("maquinas-aceitas-erro");
    erro.hidden = false;
    erro.textContent = fraseDaFalhaDaMaquina(falhaDasMaquinasAceitas);
    return;
  }
  if (aceitas.length === 0) {
    repovoar(lista, [elemento("li", "server-dispositivos-vazio", F.listaVazia)]);
    return;
  }
  repovoar(lista, aceitas.map((aceita) => {
    const linha = elemento("li");
    const caixa = elemento("div", "server-dispositivo mods-linha-gestao");
    const texto = elemento("span", "server-dispositivo-nome");
    // O estado é um chip (0.19, `Config-mods`): verde de pé, neutro desligada.
    const estado = elemento("span", "maquina-estado", aceita.ligada ? F.listaLigada : F.listaDesligada);
    estado.dataset.ligada = aceita.ligada ? "sim" : "nao";
    texto.append(
      elemento("span", "mods-id", F.listaLinha(aceita.titulo, aceita.mod_id, aceita.servidor)),
      estado,
      elemento("span", "mods-versao", [
        F.listaPedidoPor(aceita.pedido_por),
        ...((aceita.tuneis ?? []).length > 0
          ? [F.listaPortas(aceita.tuneis.map((t) => `${t.porta} (${t.protocolo})`).join(", "))]
          : []),
      ].join(" · ")),
    );
    caixa.append(texto);
    if (aceita.ligada) {
      const desligar = elemento("button", "botao-fantasma", F.desligar);
      desligar.type = "button";
      desligar.addEventListener("click", () => {
        desligarPelaLista(aceita).catch((falha) => {
          registrarNoAnfitriao("maquina", `desligar falhou — ${String(falha?.message ?? falha)}`, "aviso", aceita.mod_id);
        });
      });
      caixa.append(desligar);
    }
    const botao = elemento("button", "botao-fantasma", F.revogar);
    botao.type = "button";
    botao.addEventListener("click", () => {
      revogarUmaMaquina(aceita).catch((falha) => {
        registrarNoAnfitriao("maquina", `revogar falhou — ${String(falha?.message ?? falha)}`, "aviso", aceita.mod_id);
      });
    });
    caixa.append(botao);
    linha.append(caixa);
    return linha;
  }));
}

/** DESLIGAR na lista: o mesmo verbo da faixa, e o aceite continua guardado. */
async function desligarPelaLista(aceita) {
  const erro = $("maquinas-aceitas-erro");
  erro.hidden = true;
  try {
    await invoke("maquina_desligar", { alvo: aceita.alvo, modId: aceita.mod_id, servidor: aceita.servidor });
  } catch (falha) {
    erro.hidden = false;
    erro.textContent = fraseDaFalhaDaMaquina(falha);
  }
  await desenharMaquinasAceitas();
}

async function revogarUmaMaquina(aceita) {
  const erro = $("maquinas-aceitas-erro");
  erro.hidden = true;
  try {
    await invoke("maquina_revogar", { alvo: aceita.alvo, modId: aceita.mod_id, servidor: aceita.servidor });
  } catch (falha) {
    erro.hidden = false;
    erro.textContent = fraseDaFalhaDaMaquina(falha);
  }
  await desenharMaquinasAceitas();
}

// ------------------------------------------------------------------ os eventos

listen("maquina-pedido", ({ payload }) => {
  if (!payload || typeof payload !== "object" || !payload.oferta) {
    registrarNoAnfitriao("maquina", "um maquina-pedido sem oferta foi ignorado", "aviso");
    return;
  }
  chegouUmPedido(payload);
});

listen("maquina-estado", ({ payload }) => {
  if (!payload || typeof payload !== "object") return;
  aplicarOEstadoDaMaquina(payload);
});

// O jogo começou ou acabou neste computador: a linha da máquina passa a dizer.
listen("maquina-jogo", ({ payload }) => {
  if (!payload || typeof payload !== "object") return;
  const chave = chaveDoEvento(payload);
  jogosRodando.set(chave, payload.rodando === true);
  if (maquinasNaFaixa.get(chave)?.ligada) desenharAFaixaDeMaquinas();
});

/** Um jogo que sobrou de um app anterior, na faixa (uma linha por processo). */
function aplicarASobra(sobra) {
  if (!sobra || typeof sobra !== "object") return;
  const chave = `sobra\u0000${sobra.pid}`;
  if (maquinasNaFaixa.has(chave)) return;
  maquinasNaFaixa.set(chave, {
    alvo: sobra.alvo, modId: sobra.mod_id || "MOD", servidor: sobra.servidor,
    titulo: sobra.titulo || sobra.servidor || "um servidor de jogo",
    ligada: false, motivo: sobra.desfecho === "saiu" ? "sobra" : "sobra-forcada", falha: null, sobra,
  });
  desenharAFaixaDeMaquinas();
}

listen("maquina-sobra", ({ payload }) => aplicarASobra(payload));

relerAsMaquinasLigadas();
// As sobras que o Rust já achou antes de esta página ouvir o evento.
invoke("maquinas_sobras").then((lista) => {
  for (const sobra of Array.isArray(lista) ? lista : []) aplicarASobra(sobra);
}).catch((falha) => registrarNoAnfitriao("maquina", `maquinas_sobras falhou — ${codigoDaFalhaDaMaquina(falha)}`, "aviso"));
