// SEELE · Entry Plug — a tela de operação (`#tela-sessao`).
//
// O projetor: cada `desenhar*` recebe o snapshot inteiro e escreve a tela, sem
// estado derivado. Também a busca, a faixa de veredito, a bateria interna, o
// push-to-talk e a caixa de convite de quem hospeda — tudo que só existe
// enquanto há sessão.

"use strict";

/** O último snapshot desenhado, para não redesenhar o que não mudou. */
let desenhado = null;
/** A Linha aberta, para saber para onde vai o que se digita. */
let linhaAberta = null;
/** Se a barra de espaço já está segurando o microfone. */
let falando = false;
/** Volume por apelido, para o deslizante não pular de volta a cada redesenho. */
const volumes = new Map();
/**
 * Os casamentos da busca corrente, agrupados por índice de mensagem.
 *
 * Vem prontos do Rust. Os deslocamentos são em **caracteres** e valem sobre o
 * corpo normalizado — ver `corpoComRealce`.
 */
let casamentosPorMensagem = new Map();
/**
 * Qual mensagem e qual ocorrência dentro dela o cursor da busca está.
 *
 * `{ indice, ordinal }` — o índice da mensagem na ordem em que a tela desenha
 * e qual ocorrência dentro dela. `null` sem busca. Os dois números vêm prontos do
 * Rust — o ordinal é o `Search::ordinal_in_message` do core. Aqui não se conta
 * nada: contar seria a mesma contagem que decide o `[n/m]`, escrita de novo e
 * livre para discordar.
 */
let ocorrenciaAtual = null;

// ------------------------------------------------------------------ veredito

/**
 * A frase de um veredito de identidade, ou `null` quando não há o que dizer.
 *
 * A comparação já aconteceu — em Rust, dentro do `connect`, antes de isto ser
 * chamado. O que chega aqui é o resultado dela mais a impressão digital para
 * uma pessoa conferir por outro canal, que é a única coisa que se faz com uma
 * impressão digital (ADR 0003). Nada nesta função compara nada.
 *
 * `Known` não vira frase de propósito: repetir "a chave é a mesma de sempre" a
 * cada entrada é ensinar a não ler a linha no dia em que ela não for.
 *
 * A recusa de um convite que aponta para outra chave não chega como veredito —
 * ela derruba a conexão, e a frase dela vive em `fraseDeErro`.
 */
function fraseDoVeredito(veredito) {
  if (!veredito || veredito === "Known") return null;

  // `tofu.rs`: a casca tem que dizer o que acabou de confiar. Fixar em
  // silêncio é fixar sem ninguém saber que havia o que conferir.
  if (veredito.FirstContact) {
    return (
      "PRIMEIRO CONTATO — CHAVE FIXADA\n" +
      `${veredito.FirstContact.fingerprint}\n` +
      "Ninguém confirmou que é esta. Confira com quem hospeda, por outro canal."
    );
  }
  // É para produzir exatamente isto que o link do ADR 0006 existe. Não há nada
  // a fazer, e por isso a frase não pede nada.
  if (veredito.FirstContactVerified) {
    return (
      "PRIMEIRO CONTATO VERIFICADO — O CONVITE CONFIRMOU A CHAVE\n" +
      veredito.FirstContactVerified.fingerprint
    );
  }
  // O Dogma é o mesmo de sempre; o link é que não é dele. Ressalva, não queda.
  if (veredito.InviteDisagrees) {
    return (
      "O CONVITE NÃO CORRESPONDE A ESTE DOGMA.\n" +
      `esperada: ${veredito.InviteDisagrees.expected}\n` +
      `ofertada: ${veredito.InviteDisagrees.offered}\n` +
      "Você entrou no Dogma de sempre. O link é que não leva a ele."
    );
  }
  return null;
}

/** Acende — ou apaga — a faixa de veredito da sessão. */
function mostrarVeredito(veredito) {
  const frase = fraseDoVeredito(veredito);
  // Mostrar a faixa **antes** de escrever nela. Um `role="status"` só é lido
  // quando o texto muda com a região já visível: escrever primeiro e revelar
  // depois faz vários leitores de tela tratarem a mudança como conteúdo que
  // sempre esteve ali, e o veredito passa calado por quem mais precisa dele.
  $("veredito").hidden = frase === null;
  $("veredito-texto").textContent = frase ?? "";
}

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
    // O triângulo de abertura, desenhado (`ui/glifos.js`). Sem nome acessível:
    // ele vem imediatamente antes do nome do Cage, e anunciá-lo seria uma
    // palavra a mais em cada linha do painel.
    const abertura = elemento("span", null);
    abertura.append(glifo(cage.occupied_by_us ? "aberto" : "fechado"));
    item.append(abertura, elemento("span", null, cage.name));
    item.dataset.cage = String(cage.id);
    item.title = `${cage.pilots.length}/${cage.limit}`;
    linhas.push(item);

    // Pilotos aninhados sob o Cage que está aberto, como no `plug`.
    if (!cage.occupied_by_us) continue;
    for (const piloto of cage.pilots) {
      const linha = elemento("li", piloto.speaking ? "piloto falando" : "piloto");
      // A bolinha cheia e a vazada, desenhadas. `.presenca` continua com uma
      // célula de largura e a cor continua vindo de
      // `.lista .piloto.falando .presenca`, porque o desenho pinta com
      // `currentColor`.
      const presenca = elemento("span", "presenca");
      presenca.append(glifo(piloto.speaking ? "falando" : "silencio"));
      linha.append(
        presenca,
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

  const itens = snapshot.messages.map((mensagem, indice) => {
    const item = elemento("li", mensagem.own ? "propria" : null);
    const cabeca = elemento("div", "cabeca");
    cabeca.append(
      elemento("span", null, relogio(mensagem.at_seconds)),
      elemento("span", null, mensagem.author_nickname),
    );
    if (mensagem.edited) cabeca.append(elemento("span", "editada", "editada"));

    // O corpo **cru**, e isto não é detalhe de pintura. `.mensagens .corpo` é
    // `white-space: pre-wrap`: esta janela mostra quebra de linha e espaço
    // duplo como eles chegaram, e é essa string que a busca do outro lado da
    // ponte recebeu. Colapsar aqui deslocaria o realce em toda mensagem de mais
    // de uma linha; colapsar dos dois lados alinharia o realce achatando a
    // conversa, que é o preço errado a pagar por um índice.
    const corpo = elemento("div", "corpo");
    const aceso = ocorrenciaAtual?.indice === indice ? ocorrenciaAtual.ordinal : null;
    corpo.append(...corpoComRealce(mensagem.body, casamentosPorMensagem.get(indice), aceso));
    item.append(cabeca, corpo);
    return item;
  });

  repovoar(lista, itens);
  if (noFim) lista.scrollTop = lista.scrollHeight;
}

/**
 * Parte o corpo em pedaços aceso e apagado.
 *
 * Recebe os intervalos prontos do Rust: com dobramento de acento e de caixa o
 * frontend não teria como saber onde o casamento começou. Os deslocamentos são
 * em caracteres, não em unidades de código — daí o `[...corpo]`, que é o que
 * mantém o realce no lugar num corpo com emoji.
 *
 * `aceso` é qual destes intervalos é o do cursor, ou `null` se o cursor está
 * noutra mensagem. Sem ele todas as ocorrências saíam idênticas e o piloto não
 * enxergava onde estava dentro de uma mensagem que casa três vezes. A ordem
 * desta lista é a mesma em que o core contou, e é o que faz o índice bater.
 *
 * **Casamentos sobrepostos reemitem caractere.** Para "aa" em "aaa",
 * `occurrences` devolve `(0,2)` e `(1,3)`. Nesta função, ao contrário de
 * `ui.rs` no terminal, não há guarda para `start < cursor`: o segundo
 * casamento fatia `caracteres.slice(1, 3)` sem descontar que o índice 1 já
 * saiu no primeiro `<mark>`, e o caractere sobreposto é desenhado duas vezes —
 * "aaa" digitado pela pessoa vira "aaaa" na tela. `docs/pendencias.md` #14
 * registra isto; não é corrigido aqui.
 */
function corpoComRealce(corpo, intervalos, aceso = null) {
  if (!intervalos || intervalos.length === 0) return [document.createTextNode(corpo)];
  const caracteres = [...corpo];
  const pedacos = [];
  let cursor = 0;
  for (const [ordinal, { start, end }] of intervalos.entries()) {
    if (start > cursor) {
      pedacos.push(document.createTextNode(caracteres.slice(cursor, start).join("")));
    }
    const classe = ordinal === aceso ? "realce realce-atual" : "realce";
    pedacos.push(elemento("mark", classe, caracteres.slice(start, end).join("")));
    cursor = end;
  }
  if (cursor < caracteres.length) {
    pedacos.push(document.createTextNode(caracteres.slice(cursor).join("")));
  }
  return pedacos;
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
  // Visível primeiro, escrita depois — a mesma ordem da faixa de veredito, e
  // pela mesma razão: um `role="status"` escondido no instante em que o texto
  // muda não anuncia nada.
  faixa.hidden = false;
  $("bateria-conta").textContent = `${minutos}:${segundos}`;
  // As tentativas listadas, que a spec pede por nome. Zero ainda é informação:
  // quer dizer que a primeira está em curso.
  $("bateria-tentativas").textContent =
    bateria.attempts === 0 ? "reconectando…" : `${bateria.attempts} tentativas`;
  document.body.classList.add("na-bateria");
}

// --------------------------------------------------------------------- ações

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
    // Soltos **antes** do redesenho, não depois: ver `soltarCasamentos`.
    soltarCasamentos();
    await atualizar();
    // A lista de mensagens acabou de ser trocada inteira. Ver `refazerBusca`.
    await refazerBusca();
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

/** Ejeta e volta para a tela de entrada, sem fechar o programa. */
async function ejetar() {
  await invoke("disconnect");
  $("tela-sessao").hidden = true;
  $("tela-fim").hidden = true;
  $("tela-boot").hidden = false;
  $("convite").hidden = true;
  $("bateria").hidden = true;
  // O veredito era sobre a chave daquela sessão. Deixá-lo aceso sobre a
  // próxima seria dizer de um Dogma o que se apurou de outro.
  mostrarVeredito(null);
  document.body.classList.remove("na-bateria");
  desenhado = null;
  linhaAberta = null;
  await encerrarBusca();
  // O convite não sobrevive à sessão que ele abriu: quem sai, digita outro
  // endereço e aperta INSERT mandaria o token do Dogma anterior ao novo.
  limparConvite();
  // Quem acabou de sair de um Dogma tem que vê-lo na lista.
  await desenharVisitados();
}

/** Zera o campo, o cursor no Rust e o realce. */
async function encerrarBusca() {
  $("campo-busca").value = "";
  await invoke("busca_limpar");
  limparBusca();
}

// --------------------------------------------------------------------- busca

/**
 * Reagrupa os casamentos por mensagem, que é como o desenho os alcança.
 *
 * O cursor — qual das ocorrências está selecionada, e a regra de dar a volta nas
 * pontas — vive no Rust. Aqui não há decisão nenhuma sobre busca.
 */
function guardarCasamentos(estado) {
  casamentosPorMensagem = new Map();
  for (const casamento of estado.casamentos) {
    const lista = casamentosPorMensagem.get(casamento.message) ?? [];
    lista.push(casamento);
    casamentosPorMensagem.set(casamento.message, lista);
  }
  ocorrenciaAtual =
    estado.atual && estado.ordinal !== null && estado.ordinal !== undefined
      ? { indice: estado.atual.message, ordinal: estado.ordinal }
      : null;
}

function desenharBusca(estado) {
  // `[0/0]` e não vazio: "não achei" é informação, e um contador que some
  // parece uma busca que não rodou.
  $("busca-contador").textContent = `[${estado.posicao}/${estado.total}]`;
  guardarCasamentos(estado);
  if (desenhado) desenharMensagens(desenhado);
  if (estado.atual) {
    // A ocorrência, e não a mensagem. Rolar até a mensagem punha na tela a
    // linha certa e nada dentro dela: numa mensagem que casa três vezes,
    // avançar duas vezes rolava para o mesmo lugar e mexia só no algarismo.
    const alvo =
      $("lista-mensagens").querySelector(".realce-atual") ??
      $("lista-mensagens").children[estado.atual.message];
    alvo?.scrollIntoView({ block: "center" });
  }
}

function limparBusca() {
  casamentosPorMensagem = new Map();
  ocorrenciaAtual = null;
  $("busca-contador").textContent = "";
  if (desenhado) desenharMensagens(desenhado);
}

/** Há uma busca de pé? */
function buscaAtiva() {
  return $("campo-busca").value.trim() !== "";
}

/**
 * Solta os casamentos antes de um redesenho que troca a lista.
 *
 * `atualizar()` repinta com o que estiver no mapa, e só uma volta de IPC depois
 * é que `refazerBusca` conserta — mas o quadro do meio chega a aparecer,
 * acendendo trechos das mensagens erradas. Soltar antes troca um realce errado
 * visível por nenhum realce durante um quadro.
 */
function soltarCasamentos() {
  casamentosPorMensagem = new Map();
  ocorrenciaAtual = null;
}

/**
 * Refaz a busca sobre o histórico que está na tela agora.
 *
 * Obrigatório sempre que a lista desenhada mudar de forma — abrir outra Linha,
 * entrar ou sair de um Cage, uma mensagem editada ou apagada. Ao contrário do
 * `plug`, que recalcula o realce a partir do termo a cada desenho
 * (`seele-tui::ui`) e só guarda o cursor, esta janela guarda os deslocamentos
 * que vieram do Rust; sem um ponto de invalidação eles passariam a acender
 * trechos de mensagens que não são mais aquelas, com `[n/m]` contando uma
 * conversa que saiu da tela.
 *
 * O termo continua. O que se recalcula é todo o resto.
 */
async function refazerBusca() {
  const termo = $("campo-busca").value;
  if (termo.trim() === "") {
    await invoke("busca_limpar");
    limparBusca();
    return;
  }
  try {
    desenharBusca(await invoke("buscar", { termo }));
  } catch (falha) {
    // Sem sessão não há histórico para buscar. Não é erro de ninguém.
    if (falha !== "NotConnected") console.warn("buscar:", falha);
  }
}

// ------------------------------------------------------------------- ligação

$("form-busca").addEventListener("submit", (evento) => evento.preventDefault());

$("campo-busca").addEventListener("input", refazerBusca);

$("busca-proxima").addEventListener("click", async () =>
  desenharBusca(await invoke("busca_andar", { adiante: true })),
);
$("busca-anterior").addEventListener("click", async () =>
  desenharBusca(await invoke("busca_andar", { adiante: false })),
);
$("form-mensagem").addEventListener("submit", enviar);
$("lista-canais").addEventListener("click", alternarCanal);
$("banner-fechar").addEventListener("click", () => ($("banner").hidden = true));
$("veredito-fechar").addEventListener("click", () => ($("veredito").hidden = true));

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
    // A tecla desenhada, e com nome: ela não está ao lado de um rótulo, ela é
    // metade da instrução. Sem nome a frase chegaria a um leitor de tela como
    // "copie com C", que manda apertar a tecla errada.
    botao.replaceChildren(
      document.createTextNode("copie com "),
      glifo("comando", "Command"),
      document.createTextNode("C"),
    );
  }
});

$("botao-trocar").addEventListener("click", ejetar);

// A barra de espaço fala, exceto enquanto se digita — a mesma colisão que a TUI
// resolve mantendo o push-to-talk fora do modo de inserção (decisão D19).
window.addEventListener("keydown", (evento) => {
  // `/` foca a busca, como no terminal. Só fora de um campo de texto — uma
  // barra digitada numa mensagem é uma barra — e só com a sessão na tela, ou
  // engoliria a tecla para focar um campo que está escondido.
  if (evento.key === "/" && !digitando() && !$("tela-sessao").hidden) {
    evento.preventDefault();
    $("campo-busca").focus();
    return;
  }
  if (evento.target === $("campo-busca")) {
    if (evento.key === "Escape") {
      evento.preventDefault();
      encerrarBusca();
      $("campo-busca").blur();
      return;
    }
    if (evento.key === "Enter") {
      // Enter anda; Shift+Enter volta — o `n`/`N` do `plug`, no teclado que
      // esta janela tem.
      evento.preventDefault();
      invoke("busca_andar", { adiante: !evento.shiftKey }).then(desenharBusca);
      return;
    }
  }
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

  // Uma mensagem editada troca um corpo no lugar e uma apagada encurta a lista:
  // nos dois casos os deslocamentos guardados passam a apontar para outro texto,
  // que é a mesma falha que trocar de Linha causava, por outra porta. Mensagem
  // acrescentada não desloca nada — entra no fim —, mas o evento é um só e não
  // diz qual das três foi.
  //
  // Só com uma busca de pé, e só neste evento: refazer a busca a cada tique de
  // telemetria seria uma volta de IPC por nada, duas vezes por segundo.
  if (payload === "MessagesChanged" && buscaAtiva()) {
    soltarCasamentos();
    atualizar()
      .then(refazerBusca)
      .catch((falha) => console.warn("busca:", falha));
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
