// SEELE · Entry Plug — a casca desktop.
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

"use strict";

const { invoke } = window.__TAURI__.core;
const { listen } = window.__TAURI__.event;

const $ = (id) => document.getElementById(id);

/** O último snapshot desenhado, para não redesenhar o que não mudou. */
let desenhado = null;
/** A Linha aberta, para saber para onde vai o que se digita. */
let linhaAberta = null;
/** Se a barra de espaço já está segurando o microfone. */
let falando = false;
/** Volume por apelido, para o deslizante não pular de volta a cada redesenho. */
const volumes = new Map();

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

/**
 * A frase para um motivo de fim de sessão.
 *
 * O protocolo carrega enums justamente para que cada casca escreva as suas
 * (`specs/02-protocolo.md`). Este é o mesmo conjunto de frases do `plug`, em
 * português, porque é o mesmo produto.
 */
const MOTIVOS = {
  Incompatible: "VERSÃO INCOMPATÍVEL COM ESTE DOGMA",
  CredentialRejected: "CREDENCIAL RECUSADA",
  HandshakeTimeout: "TEMPO ESGOTADO NA SINCRONIZAÇÃO INICIAL",
  Kicked: "DESCONECTADO POR UM OPERADOR",
  Banned: "ACESSO BARRADO POR UM OPERADOR",
  DogmaFull: "DOGMA LOTADO",
  ScheduledMaintenance: "MANUTENÇÃO PROGRAMADA",
  ServerShuttingDown: "O DOGMA ESTÁ ENCERRANDO",
  Timeout: "ENLACE PERDIDO",
  ProtocolViolation: "PROTOCOLO VIOLADO",
  RateLimited: "LIMITE DE MENSAGENS EXCEDIDO",
  LinkLost: "ENLACE PERDIDO",
};

const AVISOS = {
  Mentioned: "VOCÊ FOI CHAMADO",
  SubsystemChanged: "UM SUBSISTEMA MUDOU DE ESTADO",
  SyncDegraded: "TAXA DE SINCRONIZAÇÃO EM QUEDA",
  CageEntryRefused: "ENTRADA NO CAGE RECUSADA",
  PermissionDenied: "PERMISSÃO NEGADA",
  CageFull: "CAGE LOTADO",
  OperatorNotice: "AVISO DO OPERADOR",
};

/**
 * A frase para uma falha de conexão.
 *
 * O erro chega como enum — nunca como texto — e é aqui que ele vira uma frase.
 * Um `PinChanged` carrega as duas impressões digitais porque a coisa toda é um
 * humano compará-las (ADR 0003).
 */
function fraseDeErro(erro) {
  if (typeof erro === "string") return FRASES[erro] ?? erro;
  if (erro && typeof erro === "object") {
    if (erro.PinChanged) {
      return (
        "A CHAVE DO SERVIDOR MUDOU.\n" +
        `fixada:   ${erro.PinChanged.pinned}\n` +
        `ofertada: ${erro.PinChanged.offered}\n` +
        "Confirme por outro canal antes de continuar."
      );
    }
    if (erro.Refused) {
      return MOTIVOS[erro.Refused.reason] ?? "SESSÃO RECUSADA";
    }
  }
  return FRASES[erro] ?? "FALHA DESCONHECIDA";
}

/**
 * Enum → frase. A fronteira erro→texto do produto fica aqui, e é por isso que
 * nenhuma mensagem para gente é escrita em Rust.
 */
const FRASES = {
    NotConnected: "SEM CONEXÃO",
    AlreadyConnected: "JÁ HÁ UMA SESSÃO ABERTA",
    UnresolvableHost: "NÃO CONSEGUI RESOLVER ESSE ENDEREÇO",
    Unreachable: "NADA RESPONDEU NESSE ENDEREÇO",
    HandshakeTimeout: "TEMPO ESGOTADO NA SINCRONIZAÇÃO INICIAL",
    IdentityUnavailable: "NÃO CONSEGUI LER OU GRAVAR A IDENTIDADE EM DISCO",
    NoAudioDevice: "SEM DISPOSITIVO DE ÁUDIO",
    UnknownPilot: "NÃO CONHEÇO ESSE PILOTO",
    UnknownChannel: "NÃO CONHEÇO ESSE CANAL",
    LinkLost: "ENLACE PERDIDO",

    // Hospedar aqui dentro.
    JaHospedando: "JÁ ESTOU HOSPEDANDO NESTA JANELA",
    PortaOcupada:
      "A PORTA 8383 JÁ ESTÁ EM USO.\nQuase sempre é outro SEELE aberto — feche o outro e tente de novo.",
    NaoSubiu: "NÃO CONSEGUI SUBIR O DOGMA AQUI",
};

// ------------------------------------------------------------------- desenho

function desenhar(snapshot) {
  if (!snapshot) return;

  if (snapshot.ended) {
    mostrarFim(snapshot.ended);
    return;
  }

  desenharTopo(snapshot);
  desenharDogma(snapshot);
  desenharCanais(snapshot);
  desenharMensagens(snapshot);
  desenharTelemetria(snapshot);
  desenharAviso(snapshot);

  desenhado = snapshot;
}

function desenharTopo(snapshot) {
  const padrao = $("padrao");
  padrao.dataset.padrao = snapshot.pattern;
  padrao.textContent = {
    Offline: "PADRÃO: DESLIGADO",
    Orange: "PADRÃO: LARANJA",
    Blue: "PADRÃO: AZUL",
  }[snapshot.pattern];
}

function desenharDogma(snapshot) {
  if (desenhado && desenhado.dogma === snapshot.dogma) return;
  const item = elemento("li", "aberto", snapshot.dogma || "—");
  repovoar($("lista-dogma"), [item]);
}

function desenharCanais(snapshot) {
  const linhas = [];

  for (const cage of snapshot.cages) {
    const item = elemento("li", cage.occupied_by_us ? "cage aberto" : "cage");
    item.append(
      elemento("span", null, cage.occupied_by_us ? "▼" : "▶"),
      elemento("span", null, cage.name),
    );
    item.dataset.cage = String(cage.id);
    item.title = `${cage.pilots.length}/${cage.limit}`;
    linhas.push(item);

    // Pilotos aninhados sob o Cage que está aberto, como no `plug`.
    if (!cage.occupied_by_us) continue;
    for (const piloto of cage.pilots) {
      const linha = elemento("li", piloto.speaking ? "piloto falando" : "piloto");
      linha.append(
        elemento("span", "presenca", piloto.speaking ? "●" : "○"),
        elemento("span", null, piloto.nickname + (piloto.is_self ? " (você)" : "")),
      );

      // A.T. Field e isolamento total têm marcador textual além da cor.
      if (piloto.at_field) linha.append(elemento("span", "marca-estado", "A.T."));
      else if (piloto.total_isolation) linha.append(elemento("span", "marca-estado", "SURDO"));

      const sync = elemento(
        "span",
        "sync",
        `${marcaSync(piloto.sync_band)}${String(piloto.sync_ratio).padStart(3, " ")}%`,
      );
      sync.dataset.faixa = piloto.sync_band;
      linha.append(sync);

      // Volume por pessoa (`specs/03-audio.md`). Não para nós mesmos: baixar o
      // próprio volume não faz nada, porque a própria voz nunca entra na mistura.
      if (!piloto.is_self && snapshot.audio_available) {
        const volume = document.createElement("input");
        volume.type = "range";
        volume.className = "volume";
        volume.min = "0";
        volume.max = "200";
        volume.step = "10";
        volume.value = String(volumes.get(piloto.nickname) ?? 100);
        volume.title = `volume de ${piloto.nickname}`;
        volume.dataset.piloto = piloto.nickname;
        linha.append(volume);
      }

      linhas.push(linha);
    }
  }

  for (const linha of snapshot.lines) {
    const item = elemento("li", linha.open ? "linha aberto" : "linha");
    item.append(elemento("span", null, "─"), elemento("span", null, `LINHA ${linha.name}`));
    item.dataset.linha = String(linha.id);
    linhas.push(item);
    if (linha.open) linhaAberta = linha.id;
  }

  repovoar($("lista-canais"), linhas);
}

function desenharMensagens(snapshot) {
  const lista = $("lista-mensagens");
  // Só rola sozinho se já estava no fim: puxar alguém de volta para baixo no
  // meio de uma leitura é pior do que não acompanhar.
  const noFim = lista.scrollHeight - lista.scrollTop - lista.clientHeight < 32;

  const itens = snapshot.messages.map((mensagem) => {
    const item = elemento("li", mensagem.own ? "propria" : null);
    const cabeca = elemento("div", "cabeca");
    cabeca.append(
      elemento("span", null, relogio(mensagem.at_seconds)),
      elemento("span", null, mensagem.author_nickname),
    );
    if (mensagem.edited) cabeca.append(elemento("span", "editada", "editada"));
    item.append(cabeca, elemento("div", "corpo", mensagem.body));
    return item;
  });

  repovoar(lista, itens);
  if (noFim) lista.scrollTop = lista.scrollHeight;
}

function desenharTelemetria(snapshot) {
  const tel = snapshot.telemetry;

  const sync = $("tel-sync");
  sync.textContent = `${marcaSync(tel.sync_band)} ${tel.sync_ratio}%`;
  sync.dataset.faixa = tel.sync_band;
  sync.className = "sync";

  $("tel-rtt").textContent = `${Math.round(tel.rtt_ms)}ms`;
  $("tel-jit").textContent = `${Math.round(tel.jitter_ms)}ms`;
  $("tel-loss").textContent = `${(tel.loss_fraction * 100).toFixed(1)}%`;
  $("tel-opus").textContent = tel.audio_available === false ? "—" : `${Math.round(tel.bitrate_bps / 1000)}k`;

  $("tel-local").hidden = !tel.local_fault;

  desenharEnlace(snapshot.link);

  const mudo = $("botao-mudo");
  mudo.textContent = snapshot.at_field ? "A.T. ON" : "A.T. OFF";
  mudo.dataset.ativo = snapshot.at_field ? "sim" : "nao";

  const surdo = $("botao-surdo");
  surdo.textContent = snapshot.total_isolation ? "SURDO" : "OUVINDO";
  surdo.dataset.ativo = snapshot.total_isolation ? "sim" : "nao";

  const voz = $("botao-voz");
  // "MODO:" na frente porque `TECLA` sozinho não diz que é um seletor — e um
  // seletor que ninguém reconhece como seletor é um botão que ninguém aperta.
  voz.textContent = { PushToTalk: "MODO: TECLA", VoiceActivated: "MODO: VOZ", Open: "MODO: ABERTO" }[
    snapshot.voice_mode
  ] ?? "TECLA";
  voz.disabled = !snapshot.audio_available;
  voz.dataset.ativo = snapshot.voice_mode === "Open" ? "sim" : "nao";

  const falar = $("botao-falar");
  falar.dataset.ativo = snapshot.speaking ? "sim" : "nao";
  // O rótulo é a instrução. Um botão escrito "FALAR" que não faz nada ao ser
  // clicado é pior que nenhum botão: ensina a coisa errada.
  $("falar-rotulo").textContent = snapshot.speaking
    ? "NO AR"
    : { PushToTalk: "SEGURE ESPAÇO", VoiceActivated: "FALE", Open: "MICROFONE ABERTO" }[
        snapshot.voice_mode
      ] ?? "SEGURE ESPAÇO";
  falar.disabled = !snapshot.audio_available;
  falar.title = snapshot.audio_available
    ? "segure a barra de espaço, ou este botão"
    : "esta sessão não tem áudio";
}

function desenharAviso(snapshot) {
  const banner = $("banner");
  if (!snapshot.notice) {
    banner.hidden = true;
    return;
  }
  const aviso = snapshot.notice;
  banner.hidden = false;
  banner.dataset.severidade = aviso.severity;
  $("banner-texto").textContent = aviso.operator_text ?? AVISOS[aviso.reason] ?? "AVISO";
}

/**
 * A bateria interna, desenhada sobre a sessão.
 *
 * `specs/07-tema-evangelion.md` proíbe fechar ou trocar de tela quando a
 * conexão cai: esmaece, conta, e deixa o histórico legível. Por isso isto só
 * acende uma faixa e uma classe no corpo — nada some.
 */
function desenharEnlace(link) {
  const faixa = $("bateria");
  if (!link || link === "Online") {
    faixa.hidden = true;
    document.body.classList.remove("na-bateria");
    return;
  }

  const bateria = link.InternalBattery;
  if (!bateria) return;

  const restam = Math.max(0, bateria.remaining_seconds);
  const minutos = String(Math.floor(restam / 60)).padStart(2, "0");
  const segundos = String(restam % 60).padStart(2, "0");
  $("bateria-conta").textContent = `${minutos}:${segundos}`;
  // As tentativas listadas, que a spec pede por nome. Zero ainda é informação:
  // quer dizer que a primeira está em curso.
  $("bateria-tentativas").textContent =
    bateria.attempts === 0 ? "reconectando…" : `${bateria.attempts} tentativas`;
  faixa.hidden = false;
  document.body.classList.add("na-bateria");
}

function mostrarFim(motivo) {
  $("tela-sessao").hidden = true;
  $("tela-boot").hidden = true;
  $("tela-fim").hidden = false;
  $("fim-motivo").textContent = MOTIVOS[motivo] ?? "ENLACE ENCERRADO";
}

// --------------------------------------------------------------------- ações

async function atualizar() {
  try {
    desenhar(await invoke("snapshot"));
  } catch (erro) {
    // Sem sessão. Não é uma falha: é o estado antes de conectar e depois de sair.
    if (erro !== "NotConnected") console.warn("snapshot:", erro);
  }
}

async function conectar(evento) {
  evento?.preventDefault();
  const botao = $("botao-conectar");
  const erro = $("boot-erro");

  botao.disabled = true;
  erro.hidden = true;
  // Os três subsistemas reportam enquanto a conexão acontece. Duram o tempo
  // real dela: `specs/05-cliente-tui.md` chama animação decorativa que atrasa
  // o usuário de falha de design.
  for (const id of ["sub-melchior", "sub-balthasar", "sub-casper"]) $(id).textContent = "…";

  try {
    const snapshot = await invoke("connect", {
      server: $("campo-servidor").value.trim(),
      nickname: $("campo-apelido").value.trim(),
      audio: $("campo-audio").checked,
    });

    for (const id of ["sub-melchior", "sub-balthasar", "sub-casper"]) $(id).textContent = "ok";

    $("tela-boot").hidden = true;
    $("tela-sessao").hidden = false;
    desenhar(snapshot);

    // Entrar no primeiro Cage e abrir a primeira Linha é o que um cliente
    // acabado de conectar deve fazer — chegar numa tela vazia é chegar sem
    // saber o que fazer.
    if (snapshot.cages.length > 0) await invoke("insert_plug", { cage: snapshot.cages[0].id });
    if (snapshot.lines.length > 0) await invoke("open_line", { line: snapshot.lines[0].id });
    await atualizar();
  } catch (falha) {
    for (const id of ["sub-melchior", "sub-balthasar", "sub-casper"]) $(id).textContent = "·";
    erro.textContent = fraseDeErro(falha);
    erro.hidden = false;
  } finally {
    botao.disabled = false;
  }
}

async function enviar(evento) {
  evento.preventDefault();
  const campo = $("campo-mensagem");
  const corpo = campo.value.trim();
  if (!corpo || linhaAberta === null) return;

  // Limpa antes de esperar a resposta: um campo que só esvazia depois do ida e
  // volta parece travado numa rede ruim, que é justo quando não pode parecer.
  campo.value = "";
  try {
    await invoke("send_message", { line: linhaAberta, body: corpo });
  } catch (falha) {
    campo.value = corpo;
    console.warn("send_message:", falha);
  }
}

async function alternarCanal(evento) {
  // O deslizante de volume vive dentro de uma linha de piloto; um clique nele
  // não é um clique no canal.
  if (evento.target.classList.contains("volume")) return;

  const item = evento.target.closest("li");
  if (!item) return;
  try {
    if (item.dataset.cage) {
      // Clicar no Cage em que já se está é sair dele — a mesma tecla entra e
      // sai, como no `plug`.
      const cage = Number(item.dataset.cage);
      if (item.classList.contains("aberto")) {
        await invoke("eject_plug");
      } else {
        await invoke("insert_plug", { cage });
      }
    } else if (item.dataset.linha) {
      linhaAberta = Number(item.dataset.linha);
      await invoke("open_line", { line: linhaAberta });
    }
    await atualizar();
  } catch (falha) {
    console.warn("canal:", falha);
  }
}

/**
 * Push-to-talk.
 *
 * A janela relata teclas soltas de verdade, então aqui é segurar de fato — sem
 * a trava que o ADR 0016 precisou inventar para terminais que não relatam.
 */
function segurarFala(segurando) {
  if (falando === segurando) return;
  falando = segurando;
  $("botao-falar").dataset.ativo = segurando ? "sim" : "nao";
  invoke("set_talking", { talking: segurando }).catch(() => {});
}

function digitando() {
  const ativo = document.activeElement;
  return ativo && (ativo.tagName === "INPUT" || ativo.tagName === "TEXTAREA");
}

// ------------------------------------------------------------------- ligação

$("form-conectar").addEventListener("submit", conectar);
$("form-mensagem").addEventListener("submit", enviar);
$("lista-canais").addEventListener("click", alternarCanal);
$("banner-fechar").addEventListener("click", () => ($("banner").hidden = true));

$("botao-mudo").addEventListener("click", async () => {
  const snapshot = await invoke("snapshot");
  await invoke("set_at_field", { on: !snapshot.at_field });
  await atualizar();
});

$("botao-surdo").addEventListener("click", async () => {
  const snapshot = await invoke("snapshot");
  await invoke("set_total_isolation", { on: !snapshot.total_isolation });
  await atualizar();
});

// O deslizante manda enquanto arrasta: ouvir o efeito é o que diz onde parar.
$("lista-canais").addEventListener("input", (evento) => {
  const alvo = evento.target;
  if (!alvo.classList.contains("volume")) return;
  const percent = Number(alvo.value);
  volumes.set(alvo.dataset.piloto, percent);
  invoke("set_volume", { nickname: alvo.dataset.piloto, percent }).catch((falha) => {
    console.warn("set_volume:", falha);
  });
});

// TECLA → VOZ → ABERTO → TECLA. `specs/03-audio.md` faz de push-to-talk o
// padrão porque ele nunca dispara sozinho; sair dele é sempre um ato explícito.
$("botao-voz").addEventListener("click", async () => {
  const snapshot = await invoke("snapshot");
  const proximo = { PushToTalk: "VoiceActivated", VoiceActivated: "Open", Open: "PushToTalk" }[
    snapshot.voice_mode
  ] ?? "PushToTalk";
  await invoke("set_voice_mode", { mode: proximo });
  await atualizar();
});

$("botao-falar").addEventListener("pointerdown", () => segurarFala(true));
$("botao-falar").addEventListener("pointerup", () => segurarFala(false));
$("botao-falar").addEventListener("pointerleave", () => segurarFala(false));

/**
 * Vira anfitrião: sobe o Dogma dentro deste app e entra nele.
 *
 * Duas etapas de propósito. `hospedar` põe o servidor de pé e devolve o link;
 * conectar é o caminho de sempre, com o endereço que ele devolveu. Um Dogma
 * hospedado aqui e um do outro lado do mundo entram pela mesma porta.
 */
async function hospedar() {
  const botao = $("botao-hospedar");
  const erro = $("boot-erro");
  botao.disabled = true;
  erro.hidden = true;

  try {
    const anfitriao = await invoke("hospedar");
    $("campo-servidor").value = anfitriao.aqui;
    $("convite-link").value = anfitriao.convite;
    $("convite").hidden = false;
    await conectar();
  } catch (falha) {
    erro.textContent = fraseDeErro(falha);
    erro.hidden = false;
  } finally {
    botao.disabled = false;
  }
}

$("botao-hospedar").addEventListener("click", hospedar);

$("convite-copiar").addEventListener("click", async () => {
  const campo = $("convite-link");
  // `select()` antes de tudo: se a área de transferência for negada, a pessoa
  // ainda fica com o link selecionado e copia com o teclado.
  campo.select();
  const botao = $("convite-copiar");
  try {
    await navigator.clipboard.writeText(campo.value);
    botao.textContent = "copiado";
    botao.classList.add("convite-copiado");
  } catch {
    botao.textContent = "copie com ⌘C";
  }
});

/** Ejeta e volta para a tela de entrada, sem fechar o programa. */
async function ejetar() {
  await invoke("disconnect");
  $("tela-sessao").hidden = true;
  $("tela-fim").hidden = true;
  $("tela-boot").hidden = false;
  $("convite").hidden = true;
  $("bateria").hidden = true;
  document.body.classList.remove("na-bateria");
  desenhado = null;
  linhaAberta = null;
}

$("botao-trocar").addEventListener("click", ejetar);

$("botao-voltar").addEventListener("click", async () => {
  await invoke("disconnect");
  $("tela-fim").hidden = true;
  $("tela-boot").hidden = false;
  // O `disconnect` também derruba o Dogma hospedado. A caixa some junto, ou
  // ficaria oferecendo um link que não leva mais a lugar nenhum.
  $("convite").hidden = true;
  desenhado = null;
  linhaAberta = null;
});

// A barra de espaço fala, exceto enquanto se digita — a mesma colisão que a TUI
// resolve mantendo o push-to-talk fora do modo de inserção (decisão D19).
window.addEventListener("keydown", (evento) => {
  if (evento.code === "Space" && !digitando() && !evento.repeat) {
    evento.preventDefault();
    segurarFala(true);
  }
});
window.addEventListener("keyup", (evento) => {
  if (evento.code === "Space" && !digitando()) segurarFala(false);
});
// Uma janela que perde o foco com o microfone aberto é um microfone esquecido.
window.addEventListener("blur", () => segurarFala(false));

// O core diz o que mudou; isto só redesenha. Um snapshot por evento é barato e
// evita que a tela e o estado discordem.
listen("seele://event", (evento) => {
  const payload = evento.payload;
  if (payload && typeof payload === "object" && payload.Ended) {
    mostrarFim(payload.Ended.reason);
    return;
  }
  atualizar();
});

// O relógio do topo é do relógio local, não do servidor.
setInterval(() => {
  $("relogio").textContent = new Date().toLocaleTimeString();
}, 1000);

// A telemetria muda sozinha entre eventos — nível de entrada, RTT, deriva.
setInterval(() => {
  if (!$("tela-sessao").hidden) atualizar();
}, 500);
