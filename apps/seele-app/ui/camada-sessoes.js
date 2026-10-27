// SEELE — várias sessões ao mesmo tempo, na casca (E3 da 0.17).
//
// # O que este arquivo desenha
//
// O contrato é o da fronteiras.md v2 (§6): `sessoes()` diz as sessões abertas,
// `ver_sessao` troca a vista sem derrubar ninguém, `focar_voz` leva o
// microfone de uma sessão para outra, e `meu_servidor()` diz onde esta
// máquina HOSPEDA, que é outra coisa:
//
// - **a trilha** ganha três grupos: ABERTAS (onde se conversa; cada placa com
//   as não lidas, a VOZ, o AQUI, e a volta ou o fim), NO AR (onde se hospeda)
//   e RECENTES (o histórico, sem as abertas);
// - **o operador** diz onde está a voz e oferece TRAZER A VOZ PARA CÁ — o ato
//   explícito: a sessão anterior para de receber o microfone antes de esta
//   começar (o Rust garante a ordem; a tela diz qual sala ficou para trás);
// - **a entrada** lista as conversas que continuam abertas, com a volta para
//   cada uma: ir à entrada para conectar a mais um servidor não fecha nenhuma.
//
// A casca pequena reaproveita a trilha (deitada na aba SERVIDOR)
// e o operador, e por isso não tem cópia nenhuma disto.
//
// # Sem o contrato novo
//
// Uma ponte que não responde `sessoes` (a de antes, ou uma bancada que não a
// combina) deixa `sessoesNaTela` em `null`, e nada aqui aparece: a trilha é a
// de sempre, com o atual e o histórico, e o `+` sai deste servidor como saía.
//
// # O roteamento
//
// O carimbo de cada comando, a recusa da resposta que chega depois da troca e
// o desvio dos eventos de outra sessão moram em `base.js` (`invoke`, `listen`),
// num lugar só. Daqui só sai o que é da trilha: reler `sessoes()` quando uma
// placa muda (`aoMudarUmaSessao`).

"use strict";

/** As sessões abertas, como `sessoes()` as dá, ou `null` sem o contrato. */
let sessoesNaTela = null;

/** O servidor no ar nesta máquina (`meu_servidor`), ou `null`. */
let hospedadoNaTela = null;

/** O pedido de reler, quando vários sinais chegam juntos. */
let releituraMarcada = null;

/** Quantas trocas de vista começaram: a resposta de uma velha não desenha. */
let trocasDeVista = 0;

/** A troca em curso, `{ de }`, desde o primeiro clique até a última responder. */
let trocaEmCurso = null;

/**
 * A vista que a sessão pede. **Só o metadado do Rust decide**: `ligacao` (o
 * código da ligação pessoal feita daqui) ou `onde: "Pessoal"` (atendi alguém na
 * minha sala). Nunca o nome nem o endereço, que a pessoa e o servidor escolhem.
 */
function vistaDaSessao(s) {
  return s?.ligacao != null || s?.onde === "Pessoal" ? "ligacao" : "servidor";
}

/**
 * Rotula a vista da sessão que está na tela. Só desenha: não troca o foco de
 * voz, não encerra nada e não abre conexão. Reaproveita a vista de chamada.
 */
function desenharAVistaPessoal() {
  const naVista = variasSessoes() ? sessoesNaTela.find((s) => s.na_vista) : null;
  const vista = naVista ? vistaDaSessao(naVista) : "servidor";
  $("tela-sessao").dataset.vista = vista;
  // **A vista é a da chamada que já existe**, e não uma cópia: a grade de quem
  // está na ligação, o palco da tela compartilhada e o operador (microfone,
  // dispositivos, SAIR DA SALA) são os de sempre. O CSS tira o que é de
  // servidor (os canais); a trilha, com as outras sessões, fica. Abre uma vez
  // por sessão que entra na vista pessoal, e fecha ao sair dela.
  const quem = vista === "ligacao" ? (naVista.carimbo?.sessao ?? null) : null;
  if (quem !== vistaPessoalAberta) {
    const tinha = vistaPessoalAberta !== null;
    vistaPessoalAberta = quem;
    // Cada transição (entrar, sair, voltar) é uma abertura nova.
    aberturasDaVistaPessoal += 1;
    // O chat é da abertura: trocar de sessão (ou voltar) o fecha, e a volta não
    // o reabre sozinha.
    fecharOChat(false);
    if (quem !== null && typeof abrirChamada === "function") {
      abrirAVistaPessoal(aberturasDaVistaPessoal).catch((falha) => console.warn("a chamada da ligação:", falha));
    } else if (tinha && typeof fecharChamada === "function") {
      // Já fora da vista pessoal: a grade fecha e a conversa volta.
      fecharChamada();
    }
  }
  desenharACabecaDaLigacao(naVista, vista);
}

/**
 * O cabeçalho da ligação pessoal e os rótulos da barra: LIGAÇÃO PESSOAL, o nome
 * do contato (ou o código) e o estado da ligação. Só rotula.
 */
function desenharACabecaDaLigacao(naVista, vista) {
  const pessoal = vista === "ligacao";
  $("ligacao-cabeca").hidden = !pessoal;
  const frase = $("sessao-ligacao");
  frase.hidden = !pessoal;
  frase.textContent = pessoal
    ? FRASES_DAS_SESSOES.ligacaoPessoal((naVista.nome ?? "").trim() || naVista.ligacao || nomeDaSessao(naVista))
    : "";
  const estados = FRASES_DAS_SESSOES.estadoDaLigacao;
  $("ligacao-cabeca-estado").textContent = !pessoal
    ? ""
    : naVista.enlace === "encerrada"
      ? estados.encerrada
      : naVista.enlace === "reconectando"
        ? estados.reconectando
        : naVista.na_sala_de_voz
          ? estados.naSala
          : estados.fora;
  // Feita daqui, desliga; recebida na minha sala, sai desta ligação (o mesmo
  // comando, só desta sessão, nos dois casos).
  const rotulo = naVista?.ligacao != null ? FRASES_DAS_SESSOES.desligar : FRASES_DAS_SESSOES.sairDestaLigacao;
  $("ligacao-desligar").textContent = rotulo;
  $("ligacao-barra-dica").textContent = pessoal ? FRASES_DAS_SESSOES.dicaDaBarra : "";
}

// ---------------------------------------------------------------------- o chat

/** Quem abriu o chat, para o foco voltar a ele. */
let origemDoChat = null;

function chatAberto() {
  return $("tela-sessao").dataset.chat === "aberto";
}

/**
 * Abre a conversa da sessão pessoal num painel sobreposto: é o MESMO
 * `#vista-conversa` da tela de servidor (histórico e envio reais), sem cópia.
 * Diálogo acessível: o foco entra no campo de mensagem e fica preso, o Escape
 * fecha, e o foco volta ao botão.
 */
function abrirOChat() {
  if (!naVistaPessoal() || chatAberto()) return;
  origemDoChat = $("ligacao-chat");
  const painel = $("vista-conversa");
  $("tela-sessao").dataset.chat = "aberto";
  painel.hidden = false;
  painel.setAttribute("role", "dialog");
  painel.setAttribute("aria-modal", "true");
  painel.setAttribute("aria-label", FRASES_DAS_SESSOES.chatDaLigacao);
  $("ligacao-chat").setAttribute("aria-expanded", "true");
  medirABarraDaLigacao();
  // A lista ficou sem layout enquanto escondida: relê, como a volta da grade.
  atualizar().catch((falha) => console.warn("abrir o chat:", falha));
  $("campo-mensagem").focus();
}

/**
 * Quanto da janela a barra da chamada ocupa, de baixo para cima: o painel do
 * chat, onde não há largura para dividir o palco, termina aí, e não cobre os
 * controles. Medido ao abrir e ao redimensionar.
 */
function medirABarraDaLigacao() {
  const barra = document.querySelector(".operador");
  if (!barra) return;
  const alto = Math.max(0, window.innerHeight - Math.round(barra.getBoundingClientRect().top));
  document.documentElement.style.setProperty("--ligacao-barra-alto", `${alto}px`);
}

window.addEventListener("resize", () => {
  if (chatAberto()) medirABarraDaLigacao();
});

/** Fecha o chat; `devolverOFoco` leva o foco de volta ao botão que o abriu. */
function fecharOChat(devolverOFoco) {
  const painel = $("vista-conversa");
  const estava = chatAberto();
  delete $("tela-sessao").dataset.chat;
  painel.removeAttribute("role");
  painel.removeAttribute("aria-modal");
  painel.removeAttribute("aria-label");
  $("ligacao-chat").setAttribute("aria-expanded", "false");
  // Na vista pessoal a conversa só existe dentro do chat; nas outras, é a de sempre.
  painel.hidden = naVistaPessoal();
  if (estava && devolverOFoco) origemDoChat?.focus();
  origemDoChat = null;
}

$("ligacao-chat").addEventListener("click", () => {
  if (chatAberto()) fecharOChat(true);
  else abrirOChat();
});
$("ligacao-chat-fechar").addEventListener("click", () => fecharOChat(true));

// O foco que cai fora do painel enquanto o chat está aberto (o Tab do WebKit
// não passa por todo botão, e o teclado pode sair por onde o laço do Tab não
// vê) volta para dentro dele.
document.addEventListener("focusin", (evento) => {
  if (!chatAberto()) return;
  const painel = $("vista-conversa");
  if (painel.contains(evento.target)) return;
  const primeiro = [...painel.querySelectorAll("button, input, textarea, select")].find(
    (el) => !el.disabled && el.getClientRects().length > 0,
  );
  primeiro?.focus();
});

// E o foco que se perde (vai ao corpo da página, sem cair em outro elemento)
// também volta ao último elemento do painel em que estava.
$("vista-conversa").addEventListener("focusout", (evento) => {
  if (!chatAberto()) return;
  const saiu = evento.target;
  setTimeout(() => {
    if (chatAberto() && !$("vista-conversa").contains(document.activeElement) && saiu.isConnected) saiu.focus();
  }, 0);
});

// Escape fecha (sem esperar a busca, que fecha o próprio Escape dela primeiro) e
// o Tab fica preso no painel enquanto ele está aberto.
window.addEventListener("keydown", (evento) => {
  if (!chatAberto()) return;
  const painel = $("vista-conversa");
  if (evento.key === "Escape" && evento.target !== $("campo-busca")) {
    evento.preventDefault();
    fecharOChat(true);
    return;
  }
  if (evento.key !== "Tab") return;
  const alvos = [...painel.querySelectorAll("button, input, textarea, select, [tabindex]:not([tabindex='-1'])")].filter(
    (el) => !el.disabled && el.getClientRects().length > 0,
  );
  if (alvos.length === 0) return;
  const primeiro = alvos[0];
  const ultimo = alvos[alvos.length - 1];
  if (!painel.contains(document.activeElement)) {
    evento.preventDefault();
    primeiro.focus();
  } else if (evento.shiftKey && document.activeElement === primeiro) {
    evento.preventDefault();
    ultimo.focus();
  } else if (!evento.shiftKey && document.activeElement === ultimo) {
    evento.preventDefault();
    primeiro.focus();
  }
});

/**
 * O miolo da ligação FORA DA SALA (saiu pelo SAIR DA SALA ou pelo Escape): o
 * estado dela e as três saídas. A ligação continua aberta; sem este painel a
 * grade fechada deixava o miolo vazio.
 */
let leiturasDoFora = 0;
async function desenharForaDaSala() {
  const esta = ++leiturasDoFora;
  const daAbertura = aberturasDaVistaPessoal;
  const naVista = variasSessoes() ? sessoesNaTela.find((s) => s.na_vista) : null;
  // O nome do contato; sem nome, o código da ligação.
  const quem = naVista ? (naVista.nome ?? "").trim() || naVista.ligacao || nomeDaSessao(naVista) : "";
  $("ligacao-fora-erro").hidden = true;
  $("ligacao-entrar").hidden = true;
  $("ligacao-reabrir").hidden = true;
  // Feita daqui, desliga; recebida na minha sala, sai desta ligação. O comando
  // é o mesmo dos dois lados (`encerrar_sessao`, só desta sessão): o rótulo
  // não promete encerrar para todos, que é o `encerrar_a_ligacao`, da sala.
  $("ligacao-encerrar").textContent =
    naVista?.ligacao != null ? FRASES_DAS_SESSOES.desligar : FRASES_DAS_SESSOES.sairDestaLigacao;
  $("ligacao-fora-estado").textContent = FRASES_DAS_SESSOES.lendoASala;
  // **A ocupação REAL**, lida do retrato: o painel não supõe que saiu.
  let sentado = null;
  try {
    const quadro = await invoke("snapshot");
    sentado = (quadro?.voice_rooms ?? []).some((sala) => sala.occupied_by_us);
  } catch {
    sentado = null;
  }
  // Só a leitura mais nova, DA MESMA abertura da vista pessoal que a pediu,
  // escreve. O contador sobe a cada transição (entrar, sair, voltar), então a
  // resposta atrasada de outra sessão, ou da primeira visita à mesma sessão,
  // é descartada; a vista vigente tem a leitura dela, pedida ao entrar. (Não
  // é a época da vista: ela sobe de propósito no meio da própria troca.)
  if (esta !== leiturasDoFora || daAbertura !== aberturasDaVistaPessoal) return;
  const frase =
    sentado === true
      ? FRASES_DAS_SESSOES.aindaNaSala(quem)
      : sentado === false
        ? FRASES_DAS_SESSOES.foraDaSala(quem)
        : FRASES_DAS_SESSOES.semLeitura;
  $("ligacao-fora-estado").textContent = `${frase} ${FRASES_DAS_SESSOES.soEstaSessao}`;
  // ENTRAR só para quem comprovadamente saiu; quem ainda está sentado (ou não
  // se sabe) volta à grade, sem entrar de novo.
  $("ligacao-entrar").hidden = sentado !== false;
  $("ligacao-reabrir").hidden = sentado === false;
}

/** Entra na sala da ligação (a sala tem uma só); a falha SOBE, e quem chama a mostra. */
async function entrarNaSalaDaLigacao() {
  const quadro = await invoke("snapshot");
  const sala = quadro?.voice_rooms?.[0];
  if (!sala) throw "SemSalaDaLigacao";
  await invoke("enter_voice_room", { voiceRoom: sala.id, senha: null });
}

$("ligacao-entrar").addEventListener("click", async () => {
  const erro = $("ligacao-fora-erro");
  erro.hidden = true;
  // A grade abre SÓ se a entrada deu certo; a falha aparece aqui, com a frase.
  try {
    await entrarNaSalaDaLigacao();
  } catch (falha) {
    erro.textContent = fraseDeErro(falha);
    erro.hidden = false;
    return;
  }
  await abrirChamada();
});

$("ligacao-reabrir").addEventListener("click", () => {
  abrirChamada().catch((falha) => console.warn("a chamada da ligação:", falha));
});

async function desligarALigacao() {
  const naVista = variasSessoes() ? sessoesNaTela.find((s) => s.na_vista) : null;
  if (!naVista?.carimbo) return;
  fecharOChat(false);
  // SÓ esta sessão (`encerrar_sessao` com o carimbo dela, o caminho da lista da
  // entrada). Nunca `encerrar_a_ligacao`, que fecha a sala pessoal desta máquina
  // para todos. Sai da vista antes, sem ejetar as outras.
  irParaAEntradaSemSair();
  try {
    await invoke("encerrar_sessao", { carimbo: naVista.carimbo });
  } catch (falha) {
    console.warn("encerrar a ligação:", falha);
  } finally {
    await recarregarSessoes();
  }
}

// O painel de fora e a barra chamam o MESMO gesto: sai só desta sessão.
$("ligacao-encerrar").addEventListener("click", desligarALigacao);
$("ligacao-desligar").addEventListener("click", desligarALigacao);

$("ligacao-voltar").addEventListener("click", () => {
  // Às outras sessões, que a entrada lista com a volta para cada uma.
  abrirMaisUmaSessao(null);
});

/**
 * Abre a vista pessoal pela OCUPAÇÃO REAL da sala: a grade só abre com a pessoa
 * sentada numa sala de voz desta sessão. Fora dela (a ligação recém-conectada),
 * o painel de fora, com ENTRAR NA SALA e DESLIGAR: sem auto-entrar e sem mexer
 * no foco de voz. Os guardas são os de sempre: a época e a sessão da vista.
 */
async function abrirAVistaPessoal(abertura) {
  // Primeiro o painel (nunca o miolo vazio); a grade o substitui se a leitura
  // confirmar que a pessoa está na sala.
  mostrarOQueFicaForaDaGrade();
  let sentado = null;
  try {
    const quadro = await invoke("snapshot");
    sentado = (quadro?.voice_rooms ?? []).some((sala) => sala.occupied_by_us);
  } catch {
    sentado = null;
  }
  // Amarrada à sessão da vista pessoal que a pediu: se a vista saiu dela (ou
  // foi para outra ligação) enquanto o retrato vinha, o resultado é descartado.
  if (abertura !== aberturasDaVistaPessoal) return;
  if (sentado === true) await abrirChamada();
}

/** A sessão cuja vista pessoal abriu a grade da chamada, ou `null`. */
let vistaPessoalAberta = null;

/** Sobe a cada transição da vista pessoal: guarda monotônico das leituras. */
let aberturasDaVistaPessoal = 0;

/** O nome que a tela dá a uma sessão. */
function nomeDaSessao(s) {
  return (s?.nome ?? "").trim() || s?.endereco || "servidor sem nome";
}

/**
 * O nome da sessão que tem a voz, quando ela NÃO é a mostrada; senão `null`.
 * É o que o operador usa para dizer aonde a tecla e o microfone vão.
 */
function vozNoutraSessao() {
  if (!variasSessoes()) return null;
  const comAVoz = sessoesNaTela.find((s) => s.foco_de_voz);
  const daVista = sessoesNaTela.find((s) => s.na_vista);
  return comAVoz && daVista && comAVoz !== daVista ? nomeDaSessao(comAVoz) : null;
}

/** Quem quer redesenhar quando as sessões mudam (a casca do celular). */
const aoMudaremAsSessoes = [];

/**
 * O que se vê, em palavras, para quem não tem a trilha à vista: «<servidor>
 * · N ABERTAS», ou `null` com uma sessão só.
 */
function rotuloDaSessaoNaVista() {
  if (!variasSessoes() || sessoesNaTela.length < 2) return null;
  const daVista = sessoesNaTela.find((s) => s.na_vista);
  return daVista ? FRASES_DAS_SESSOES.naVistaDoCelular(nomeDaSessao(daVista), sessoesNaTela.length) : null;
}

/** As sessões em modo de várias: o contrato respondeu com uma lista. */
function variasSessoes() {
  return Array.isArray(sessoesNaTela);
}

/** Os endereços abertos, que o histórico da trilha não repete. */
function alvosAbertos() {
  return new Set(variasSessoes() ? sessoesNaTela.map((s) => s.endereco) : []);
}

/** `base.js` chama ao desviar um evento ou recusar uma resposta de outra sessão. */
function aoMudarUmaSessao() {
  if (releituraMarcada) return;
  releituraMarcada = setTimeout(() => {
    releituraMarcada = null;
    recarregarSessoes().catch((falha) => console.warn("sessões:", falha));
  }, 150);
}

/** Relê as sessões e o servidor no ar, e redesenha o que depende deles. */
async function recarregarSessoes() {
  let lidas = null;
  try {
    lidas = await invoke("sessoes");
  } catch {
    lidas = null;
  }
  sessoesNaTela = Array.isArray(lidas) ? lidas : null;
  try {
    hospedadoNaTela = await invoke("meu_servidor");
  } catch {
    hospedadoNaTela = null;
  }
  if (variasSessoes()) {
    sessoesConhecidas = new Set(sessoesNaTela.map((s) => s.carimbo?.sessao));
    const chavesDeIcones = new Set(sessoesNaTela.map((s) => chaveDoIcone(s.carimbo)));
    for (const chave of iconesDasSessoes.keys()) if (!chavesDeIcones.has(chave)) iconesDasSessoes.delete(chave);
    // A escolha de palco guardada de uma sessão que já não está aberta sai.
    if (typeof podarOPalco === "function") podarOPalco(sessoesNaTela.map((s) => s.carimbo));
    for (const s of sessoesNaTela) if (s.carimbo) anotarGeracao(s.carimbo);
    // A janela abriu com uma sessão já na vista (a recarga da página, ou a
    // bancada): a vista vem da lista.
    const naVista = sessoesNaTela.find((s) => s.na_vista);
    if (naVista && vistaDaJanela.sessao === null) porNaVista(naVista.carimbo);
  }
  desenharSessoes();
  desenharAVozDasSessoes();
  desenharSessoesNaEntrada();
  desenharAVistaPessoal();
}

// ----------------------------------------------------------------- a trilha

/** Uma placa da trilha: a sigla (ou a imagem), as marcas, e o nome inteiro. */
function placaDaSessao(s) {
  const linha = elemento("li");
  const botao = elemento("button", "trilha-item trilha-outro trilha-sessao");
  botao.type = "button";
  botao.dataset.sessao = String(s.carimbo?.sessao ?? "");
  botao.dataset.enlace = s.enlace ?? "online";
  const nome = nomeDaSessao(s);
  const aqui = s.onde && s.onde !== "Outro";
  const rotulo = FRASES_DAS_SESSOES.placa({
    nome: s.nome, endereco: s.endereco, enlace: s.enlace, naoLidas: s.nao_lidas ?? 0,
    voz: s.foco_de_voz, aqui, naVista: s.na_vista, sala: s.na_sala_de_voz,
  });
  vestirItemDaTrilha(botao, s.nome ? sigla(s.nome) : siglaDoAlvo(s.endereco ?? ""), rotulo, iconeDaSessao(s.carimbo));
  if (s.na_vista) {
    botao.classList.add("trilha-atual");
    botao.setAttribute("aria-current", "true");
  }
  // As marcas: palavra curta e número, e nunca só a cor.
  if (s.foco_de_voz) botao.append(elemento("span", "trilha-marca trilha-marca-voz", FRASES_DAS_SESSOES.marcaVoz));
  if (aqui) botao.append(elemento("span", "trilha-marca trilha-marca-aqui", FRASES_DAS_SESSOES.marcaAqui));
  if ((s.nao_lidas ?? 0) > 0) {
    botao.append(elemento("span", "trilha-naolidas", FRASES_DAS_SESSOES.naoLidas(s.nao_lidas)));
  } else if (s.mudou) {
    botao.append(elemento("span", "trilha-naolidas trilha-mudou", "•"));
  }
  if (s.enlace === "reconectando") botao.append(elemento("span", "trilha-marca trilha-marca-enlace", "↻"));
  if (s.enlace === "encerrada") botao.append(elemento("span", "trilha-marca trilha-marca-enlace", "×"));
  for (const marca of botao.querySelectorAll(".trilha-marca, .trilha-naolidas")) marca.setAttribute("aria-hidden", "true");
  linha.append(botao);
  return linha;
}

/** A placa do servidor no ar nesta máquina, no grupo NO AR. */
function placaDoHospedado(meu) {
  const linha = elemento("li");
  const botao = elemento("button", "trilha-item trilha-outro trilha-hospedado");
  botao.type = "button";
  const nome = meu.nome || "O SEU SERVIDOR";
  vestirItemDaTrilha(botao, sigla(nome), FRASES_DAS_SESSOES.hospedando(nome), null);
  botao.append(elemento("span", "trilha-marca trilha-marca-noar", FRASES_DAS_SESSOES.marcaNoAr));
  botao.lastChild.setAttribute("aria-hidden", "true");
  botao.addEventListener("click", () => {
    // Já há uma conversa nele: é ela que se mostra. Senão, abre mais uma.
    irAoServidorHospedado(meu).catch((falha) => console.warn("ver o hospedado:", falha));
  });
  linha.append(botao);
  return linha;
}

/** Os três grupos da trilha, ou a trilha de sempre sem o contrato. */
function desenharSessoes() {
  const varias = variasSessoes();
  const grupos = ["trilha-sessoes-titulo", "trilha-sessoes", "trilha-recentes-titulo"];
  for (const id of grupos) $(id).hidden = !varias;
  $("trilha-server").hidden = varias;
  // O `+` diz o que custa: com várias sessões, nada (ele soma); sem, sair.
  $("trilha-adicionar").title = varias
    ? "abre mais uma conversa pela entrada, sem fechar as abertas"
    : "sai deste servidor e volta à entrada para você digitar outro endereço";
  const noAr = varias && hospedadoNaTela;
  $("trilha-hospedando-titulo").hidden = !noAr;
  $("trilha-hospedando").hidden = !noAr;
  if (!varias) return;
  repovoar($("trilha-sessoes"), sessoesNaTela.map(placaDaSessao));
  repovoar($("trilha-hospedando"), noAr ? [placaDoHospedado(hospedadoNaTela)] : []);
  // O histórico sem as abertas: a trilha redesenha na próxima volta.
  trilhaDesenhada = null;
  if (typeof desenhado !== "undefined" && desenhado) desenharTrilha(desenhado);
  $("trilha-recentes-titulo").hidden = $("trilha-outros").childElementCount === 0;
}

$("trilha-sessoes").addEventListener("click", (evento) => {
  const botao = evento.target.closest("button[data-sessao]");
  if (!botao || botao.getAttribute("aria-current") === "true") return;
  verSessao(Number(botao.dataset.sessao)).catch((falha) => console.warn("ver sessão:", falha));
});

// --------------------------------------- o estado transitório, por sessão
//
// **O que a pessoa estava fazendo numa sessão é DELA, e não da vista**
// (requisito da revisão do Codex). O campo, os anexos e a busca são um só na
// tela; com várias sessões, cada uma tem os seus:
//
// - o rascunho de cada canal e o campo, como estavam;
// - o anexo escolhido e ainda não mandado;
// - a busca aberta e o termo dela.
//
// Trocar de A para B guarda o de A **antes** de qualquer espera — no mesmo
// tique do clique — e esvazia a tela pelo mesmo `esquecerOEstadoDaSessao` da
// saída, que já prende (F1-vazamento, lote I-b) que o texto e o arquivo de uma
// sessão não vão no primeiro Enter da seguinte. Voltar a A põe tudo de volta:
// nada se perde calado.
//
// Os diálogos que pertencem a uma sessão fecham na troca: a moderação (um ato
// armado sobre alguém de A) e a pergunta dos MODs de uma entrada (o aceite de
// um conjunto que não é de B). Os MODs da vista param, como numa saída
// (fronteiras.md §1: os comandos de MOD são da vista). Não há citação nem
// resposta a mensagem neste produto; quando houver, ela entra aqui.

/** O estado guardado de cada sessão que saiu da vista, pelo `SessaoId`. */
const estadoPorSessao = new Map();

/**
 * **A faixa do veredito de cada sessão**, pelo `SessaoId`: `{ geracao, frase }`.
 *
 * Achado do CUA com três sessões: a faixa era uma só na janela, escrita pelo
 * último `connect`, e na vista de A mostrava PRIMEIRO CONTATO com a impressão
 * de B. Fora de `estadoPorSessao` porque ela não é guardada na saída da vista:
 * nasce na entrada (`mostrarVeredito`) e morre quando a pessoa a dispensa.
 */
const vereditoPorSessao = new Map();

/** A frase que a faixa acabou de receber, guardada com a sessão da vista. */
function guardarOVereditoDaVista(frase) {
  const sessao = vistaDaJanela.sessao;
  if (sessao === null) return;
  if (frase === null) vereditoPorSessao.delete(sessao);
  else vereditoPorSessao.set(sessao, { geracao: vistaDaJanela.geracao, frase });
}

/**
 * A faixa da sessão que volta à vista, se foi apurada NESTA geração: uma
 * reconexão é outro aperto de mão, e o veredito de antes não fala dele.
 */
function vereditoDaSessao(sessao, geracao) {
  const guardado = vereditoPorSessao.get(sessao);
  return guardado && guardado.geracao === geracao ? guardado.frase : null;
}

/** Guarda, de forma síncrona, o que a sessão mostrada tem na tela. */
function guardarOEstadoDaVista(sessao) {
  if (sessao === null || sessao === undefined) return;
  // Só uma tela desenhada daquela sessão tem o que guardar: a de transição
  // (escondida, ou já esvaziada) nunca sobrescreve o que ficou guardado.
  if ($("tela-sessao").hidden || desenhado === null) return;
  guardarRascunho();
  estadoPorSessao.set(sessao, {
    rascunhos: new Map(rascunhos),
    campo: $("campo-mensagem").value,
    anexo: anexoPendente,
    busca: { aberta: !$("form-busca").hidden, termo: $("campo-busca").value },
  });
}

/** Põe de volta o que a sessão tinha, depois de ela ser desenhada. */
function restaurarOEstadoDaVista(sessao) {
  // Os cargos guardam o deles (`cargos.js`): reabrem como estavam, se estavam.
  if (typeof cargosAoVoltarAVista === "function") cargosAoVoltarAVista(sessao);
  // E o palco, a escolha de transmissão desta sessão nesta geração.
  if (typeof restaurarOPalco === "function") restaurarOPalco(sessao, vistaDaJanela.geracao);
  // E a faixa do veredito desta sessão, e não a da última que entrou.
  pintarVeredito(vereditoDaSessao(sessao, vistaDaJanela.geracao));
  const guardado = estadoPorSessao.get(sessao);
  estadoPorSessao.delete(sessao);
  if (!guardado) return;
  rascunhos.clear();
  for (const [linha, texto] of guardado.rascunhos) rascunhos.set(linha, texto);
  restaurarRascunho();
  // O campo vale mais que o rascunho do canal: é o que estava escrito.
  if (guardado.campo && $("campo-mensagem").value !== guardado.campo) {
    $("campo-mensagem").value = guardado.campo;
  }
  if (guardado.anexo) guardarAnexo(guardado.anexo);
  if (guardado.busca.aberta) {
    $("form-busca").hidden = false;
    $("botao-buscar").setAttribute("aria-expanded", "true");
    $("campo-busca").value = guardado.busca.termo;
  }
}

/** Fecha o que pertence à sessão que sai da vista. */
function fecharOQueEDaSessao() {
  // Os cargos (G): a camada aberta, a aba e o rascunho ficam guardados com a
  // sessão que sai (a vista ainda é ela), e voltam com ela.
  if (typeof cargosAoSairDaVista === "function") cargosAoSairDaVista(vistaDaJanela.sessao);
  // A barra da busca: o `esquecerOEstadoDaSessao` zera o termo, e a barra
  // aberta ficava à vista em B, vazia, como se fosse uma busca de B.
  $("form-busca").hidden = true;
  $("botao-buscar").setAttribute("aria-expanded", "false");
  if (typeof abandonarModeracao === "function") abandonarModeracao();
  if (typeof fecharAceiteDeMods === "function" && !$("mods-aceite").hidden) fecharAceiteDeMods();
  if (typeof encerrarOAmbienteDosMods === "function") encerrarOAmbienteDosMods();
  // O palco: a imagem da sessão que sai da vista não fica na tela da outra,
  // e a ESCOLHA dela (VER, NÃO VER) fica guardada com ela — a sessão continua.
  if (typeof ocultarOPalco === "function") ocultarOPalco(vistaDaJanela.sessao, vistaDaJanela.geracao);
  // A faixa do veredito: é da chave da sessão que sai, e fica guardada com ela.
  pintarVeredito(null);
}

/**
 * **Troca a vista para uma sessão**: nenhuma conexão cai. O estado de topo da
 * tela é da sessão mostrada (o canal aberto, o rascunho, a conversa): o da que
 * sai é GUARDADO e a tela esvaziada, no mesmo tique do clique; o da que entra
 * volta depois de ela ser desenhada.
 */
async function verSessao(sessao) {
  const esta = ++trocasDeVista;
  // **O estado da que sai é guardado UMA vez, no começo da primeira troca**
  // (revisão do Codex, defeito 3). Um segundo clique antes de a primeira troca
  // responder (A → B → C) achava a tela já vazia e sobrescrevia o rascunho
  // guardado de A com nada. Enquanto há troca em curso, a tela é de transição,
  // e nada dela é guardado.
  if (trocaEmCurso === null) {
    const saindo = vistaDaJanela.sessao;
    trocaEmCurso = { de: saindo };
    if (saindo !== null && saindo !== sessao) {
      // **Antes da espera**: daqui até o `ver_sessao` responder, um Enter no
      // campo não pode levar o texto de A. A tela fica vazia, e o texto guardado.
      guardarOEstadoDaVista(saindo);
      fecharOQueEDaSessao();
      esquecerOEstadoDaSessao();
    }
  }
  let entrada;
  try {
    entrada = await invoke("ver_sessao", { sessao });
  } catch (falha) {
    // Uma troca superada por outra mais nova não faz nada: a mais nova decide.
    if (falha?.VistaSuperada || esta !== trocasDeVista) return;
    // A última troca falhou: a tela volta para a sessão de onde se saiu, com
    // o que era dela.
    const de = trocaEmCurso?.de ?? null;
    trocaEmCurso = null;
    if (de !== null && vistaDaJanela.sessao === de) restaurarOEstadoDaVista(de);
    throw falha;
  }
  if (esta !== trocasDeVista) return;
  trocaEmCurso = null;
  for (const tela of document.querySelectorAll("section.tela")) tela.hidden = true;
  $("tela-sessao").hidden = false;
  entrarNaGeracao(entrada?.geracao ?? entrada?.vista ?? 0);
  if (entrada?.snapshot) desenhar(entrada.snapshot);
  restaurarOEstadoDaVista(sessao);
  trilhaDesenhada = null;
  await recarregarSessoes();
  abrirTela("tela-sessao", FRASES_DAS_SESSOES.trocando(nomeDaSessao(sessoesNaTela?.find((s) => s.carimbo?.sessao === sessao))));
}

/**
 * Abre mais uma sessão, pela entrada: as outras ficam conectadas. A entrada é
 * o caminho que já diz tudo o que pode dar errado numa chegada (a etapa, a
 * portaria, a recusa), e ela mostra as conversas abertas para voltar.
 */
function abrirMaisUmaSessao(alvo, apelido) {
  irParaAEntradaSemSair();
  if (alvo) conectar(alvo, apelido).catch((falha) => console.warn("conectar:", falha));
}

/**
 * Leva à entrada **sem sair de sessão nenhuma**: guarda o que a vista tinha e
 * mostra a entrada, onde a etapa, a recusa e o CANCELAR de uma chegada nova
 * aparecem. Quem soma uma sessão (o `+`, o ATENDER, o LIGAR de dentro de uma
 * sessão) passa por aqui, e nunca por `ejetar`, que derruba a sessão na vista.
 */
function irParaAEntradaSemSair() {
  // O que a sessão mostrada tinha fica guardado com ela, e a tela vazia: a
  // entrada nova é desenhada sem o rascunho nem o anexo de outra.
  const saindo = vistaDaJanela.sessao;
  if (saindo !== null && !$("tela-sessao").hidden) {
    guardarOEstadoDaVista(saindo);
    fecharOQueEDaSessao();
    esquecerOEstadoDaSessao();
  }
  for (const tela of document.querySelectorAll("section.tela")) tela.hidden = true;
  $("tela-boot").hidden = false;
  voltarParaTela("tela-boot");
  desenharSessoesNaEntrada();
}

// ------------------------------------------------------------------- a voz

/** A linha do operador: onde está a voz, e o TRAZER A VOZ PARA CÁ. */
function desenharAVozDasSessoes() {
  // Com a voz aberta em alguma sessão, nada se anima na entrada (`camada-sala.css`).
  document.documentElement.dataset.vozAberta =
    variasSessoes() && sessoesNaTela.some((s) => s.foco_de_voz && s.na_sala_de_voz) ? "sim" : "nao";
  const bloco = $("voz-foco");
  const varias = variasSessoes() && sessoesNaTela.length > 1;
  bloco.hidden = !varias;
  // Quem mais desenha a partir das sessões (a casca do celular põe esta linha
  // debaixo do topo dela, e diz em toda aba qual servidor se vê): avisado
  // aqui, sem este arquivo saber de casca nenhuma.
  for (const aoMudar of aoMudaremAsSessoes) aoMudar();
  if (!varias) return;
  const daVista = sessoesNaTela.find((s) => s.na_vista);
  const comAVoz = sessoesNaTela.find((s) => s.foco_de_voz);
  const trazer = $("voz-foco-trazer");
  let frase;
  if (comAVoz && daVista && comAVoz === daVista) {
    frase = FRASES_DAS_SESSOES.vozAqui(comAVoz.na_sala_de_voz);
    bloco.dataset.voz = "aqui";
  } else if (comAVoz) {
    frase = FRASES_DAS_SESSOES.vozEmOutro(nomeDaSessao(comAVoz), comAVoz.na_sala_de_voz);
    bloco.dataset.voz = "noutro";
  } else {
    frase = FRASES_DAS_SESSOES.vozNinguem;
    bloco.dataset.voz = "nenhum";
  }
  $("voz-foco-frase").textContent = frase;
  trazer.hidden = !daVista || comAVoz === daVista || daVista.enlace === "encerrada";
}

$("voz-foco-trazer").addEventListener("click", async () => {
  const daVista = (sessoesNaTela ?? []).find((s) => s.na_vista);
  if (!daVista) return;
  const trazer = $("voz-foco-trazer");
  trazer.disabled = true;
  try {
    // O carimbo é o da sessão em que o botão foi apertado, escrito aqui, e não
    // o da vista no instante da resposta.
    const troca = await invoke("focar_voz", { carimbo: daVista.carimbo });
    anunciar(FRASES_DAS_SESSOES.trouxe(troca?.saiu_da_sala ?? null));
  } catch (falha) {
    $("voz-foco-frase").textContent = fraseDeErro(falha);
  } finally {
    trazer.disabled = false;
    await recarregarSessoes();
  }
});

// ---------------------------------------------- a queda que volta sozinha

/**
 * **«Você ficou para trás; reconectando…»** (o `CaiuComMotivo` do B: a
 * despedida recuperável, carimbada pela sessão e pela geração que caiu; na
 * web, o mesmo evento, emitido pela ponte). A conexão volta sozinha com
 * geração nova, e não há tela de fim: uma linha discreta diz o que houve, e
 * some quando a geração nova da MESMA sessão está no ar.
 *
 * Por sessão: `caidas` guarda, para cada sessão que caiu, a geração que caiu e
 * o motivo. A linha só aparece para a sessão na vista; a das outras é a placa.
 */
const caidas = new Map();

function desenharALinhaDaQueda() {
  const linha = $("sessao-ficou-para-tras");
  const queda = caidas.get(vistaDaJanela.sessao);
  linha.hidden = !queda;
  linha.textContent = queda
    ? FRASES_DAS_SESSOES.ficouParaTras[queda.motivo] ?? FRASES_DAS_SESSOES.ficouParaTras.outro
    : "";
}

/** O retrato da vista: a geração nova no ar apaga a linha da que caiu. */
function desenharFicouParaTras(snapshot) {
  const sessao = snapshot?.carimbo?.sessao ?? vistaDaJanela.sessao;
  const queda = caidas.get(sessao);
  if (queda) {
    const geracao = snapshot?.carimbo?.geracao ?? vistaDaJanela.geracao ?? 0;
    if (snapshot?.link === "Online" && geracao > queda.geracao) caidas.delete(sessao);
  }
  desenharALinhaDaQueda();
}

listen("seele://event", ({ payload }) => {
  const caiu = payload?.CaiuComMotivo;
  if (!caiu || vistaDaJanela.sessao === null) return;
  // O `listen` da casca só entrega aqui o evento da sessão na vista, e da
  // geração de pé: é ela que caiu.
  caidas.set(vistaDaJanela.sessao, { geracao: vistaDaJanela.geracao ?? 0, motivo: caiu.reason ?? null });
  desenharALinhaDaQueda();
}).catch(() => {});

// ----------------------------------------------------------------- a entrada

/** As conversas abertas, na entrada, com a volta para cada uma. */
function desenharSessoesNaEntrada() {
  const bloco = $("boot-sessoes");
  const abertas = variasSessoes() ? sessoesNaTela : [];
  bloco.hidden = abertas.length === 0;
  if (abertas.length === 0) return;
  $("boot-sessoes-frase").textContent = FRASES_DAS_SESSOES.abertasNaEntrada(abertas.length);
  repovoar($("boot-sessoes-lista"), abertas.map((s) => {
    const item = elemento("li", "boot-sessao");
    const texto = elemento("span", "boot-sessao-texto");
    texto.append(elemento("span", "boot-sessao-nome", nomeDaSessao(s)));
    texto.append(elemento("span", "boot-sessao-sub", FRASES_DAS_SESSOES.placa({
      nome: null, endereco: s.endereco, enlace: s.enlace, naoLidas: s.nao_lidas ?? 0,
      voz: s.foco_de_voz, aqui: s.onde && s.onde !== "Outro", naVista: false, sala: s.na_sala_de_voz,
    })));
    item.append(texto);
    const voltar = elemento("button", "botao-fantasma", FRASES_DAS_SESSOES.voltarA);
    voltar.type = "button";
    voltar.setAttribute("aria-label", `${FRASES_DAS_SESSOES.voltarA} a ${nomeDaSessao(s)}`);
    voltar.addEventListener("click", () => verSessao(s.carimbo.sessao).catch((falha) => console.warn("ver sessão:", falha)));
    // **Sair de uma conversa sem abri-la** (`encerrar_sessao`, pelo carimbo
    // dela): as outras continuam, e sair do servidor que esta máquina hospeda
    // não o tira do ar — quem derruba é o PARAR DE HOSPEDAR.
    const sair = elemento("button", "botao-fantasma", FRASES_DAS_SESSOES.sairDesta);
    sair.type = "button";
    sair.setAttribute("aria-label", FRASES_DAS_SESSOES.sairDe(nomeDaSessao(s), s.onde === "Guardado"));
    sair.title = sair.getAttribute("aria-label");
    sair.addEventListener("click", async () => {
      sair.disabled = true;
      try {
        await invoke("encerrar_sessao", { carimbo: s.carimbo });
      } catch (falha) {
        console.warn("encerrar sessão:", falha);
      } finally {
        await recarregarSessoes();
      }
    });
    item.append(voltar, sair);
    return item;
  }));
}

// ------------------------------------------------------------ os sinais

// O Rust avisa quando uma placa muda (§4: `seele://sessoes`); o laço lento é
// a rede de segurança de uma ponte que ainda não avisa.
listen("seele://sessoes", () => aoMudarUmaSessao()).catch(() => {});
setInterval(() => {
  if (!$("tela-sessao").hidden || !$("tela-boot").hidden) aoMudarUmaSessao();
}, 2000);
new MutationObserver(() => {
  if (!$("tela-boot").hidden) desenharSessoesNaEntrada();
}).observe($("tela-boot"), { attributes: true, attributeFilter: ["hidden"] });
recarregarSessoes().catch(() => {});
// Uma vez mais quando a página termina de carregar: quem se inscreveu em
// `aoMudaremAsSessoes` (scripts carregados depois deste) recebe o primeiro.
document.addEventListener("DOMContentLoaded", () => {
  recarregarSessoes().catch(() => {});
});
