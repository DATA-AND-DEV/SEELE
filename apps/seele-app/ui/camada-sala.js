// SEELE — a sala pessoal na tela: a linha da sala, o SEU CÓDIGO, «receber
// ligações» e a agenda (tarefas 23 e 29 da F7; spec de 23/09, «"SEU CÓDIGO" e a
// agenda» e «Erros e frases», «Quem recebe»).
//
// **A linha da sala só fala quando há o que dizer.** A sala no ar e achável não
// escreve nada, e a entrada e a sessão continuam como eram. A frase de cada
// estado é do `frases.js` (`fraseDaSala`). Ela mora num elemento próprio na
// entrada (`#boot-sala-estado`) e no operador (`#sala-estado`), e não no meio de
// um texto compartilhado: as cascas do navegador escondem a peça inteira.
//
// **O PERFIL ganha o bloco da sala** quando o `meu_codigo` responde. Sem
// resposta (o navegador recusa, e o duble dos roteiros não sabe), o bloco fica
// escondido, e o perfil é o de sempre. As ações do bloco valem na hora, sem o
// SALVAR, que é do apelido: por isso ele mora depois do formulário.
//
// **O navegador não tem sala.** Toda peça daqui leva `data-sala-pessoal`, e
// toda chamada aceita a recusa da ponte do navegador sem estourar.

"use strict";

/** O último estado da sala que chegou, ou `null` antes do primeiro. */
let estadoDaSala = null;

/**
 * A regra nasceu por este botão: a frase de sucesso e o botão escondido valem
 * até a sala subir de novo. O estado da sala é reanunciado a cada mudança do
 * registro com a leitura do firewall de quando ela subiu, e sem isto um
 * reanúncio devolveria o botão e a frase de antes à regra que já existe.
 */
let regraDoFirewallCriada = false;

/** Escreve a frase da sala onde a pessoa está, ou esconde. */
function desenharEstadoDaSala() {
  const frase = fraseDaSala(estadoDaSala);
  for (const id of ["boot-sala-estado", "sala-estado", "perfil-sala-estado"]) {
    const linha = $(id);
    linha.textContent = frase ?? "";
    linha.hidden = !frase;
  }
  // O bloco da entrada: diz a verdade da sala em todo estado.
  $("boot-codigo-estado").textContent = fraseDoCodigo(estadoDaSala);
  $("boot-codigo").dataset.recebe =
    estadoDaSala?.estado === "NoAr" && estadoDaSala.registro?.registro === "Registrado"
      ? "sim"
      : "nao";

  // «Receber ligações» está marcado em todo estado menos o desligado: a sala
  // subindo, no ar ou que falhou ao subir são todas «quero receber».
  $("perfil-receber").checked = estadoDaSala?.estado !== "Desligada";

  // O firewall do Windows (D-sala-1): o motivo que o Rust leu, a frase com o
  // que fazer, o que a 8385 é — e o botão que pede a regra ao Windows **só
  // quando ele resolve** (`abrir_resolve`): na cópia portátil e no bloqueio do
  // Windows ele não muda nada, e a frase diz o que fazer no lugar dele.
  const firewall = estadoDaSala?.estado === "NoAr" ? estadoDaSala.firewall : null;
  if (!regraDoFirewallCriada) {
    $("perfil-sala-firewall-frase").textContent = firewall
      ? `${firewall.frase} ${SALA_PESSOAL.firewall}`
      : "";
  }
  $("perfil-sala-firewall-abrir").hidden = regraDoFirewallCriada || firewall?.abrir_resolve !== true;
  $("perfil-sala-firewall").hidden = !firewall;
}

/** A direção da última ligação, numa palavra que a agenda mostra. */
const DIRECOES_DA_AGENDA = Object.freeze({
  liguei: "você ligou",
  recebi: "atendida",
  perdida: "perdida",
});

/** Uma pessoa da agenda: o nome que eu dei, o apelido dela e o código. */
function linhaDaAgenda(contato) {
  const linha = document.createElement("li");
  linha.className = "perfil-agenda-linha";

  const nome = document.createElement("input");
  nome.className = "perfil-agenda-nome";
  nome.value = contato.nome ?? "";
  nome.maxLength = 48;
  nome.spellcheck = false;
  nome.placeholder = "dar um nome";
  nome.setAttribute("aria-label", `Nome para ${contato.codigo}`);
  nome.addEventListener("change", () => {
    invoke("dar_nome_na_agenda", { codigo: contato.codigo, nome: nome.value.trim() })
      .then(() => Promise.all([desenharAgenda(), desenharAmigos()]))
      .catch((falha) => mostrarErroDaAgenda(falha));
  });

  const quem = document.createElement("span");
  quem.className = "perfil-agenda-quem";
  const partes = [];
  // O apelido entre aspas: é o que **ela** diz, e só o código é prova.
  if (contato.apelido) partes.push(`«${contato.apelido}»`);
  partes.push(contato.codigo);
  if (contato.direcao) partes.push(DIRECOES_DA_AGENDA[contato.direcao] ?? contato.direcao);
  quem.textContent = partes.join(" · ");

  const bloquear = document.createElement("button");
  bloquear.type = "button";
  bloquear.className = "botao-fantasma";
  bloquear.textContent = contato.bloqueado ? "DESBLOQUEAR" : "BLOQUEAR";
  bloquear.addEventListener("click", () => {
    invoke("bloquear", { codigo: contato.codigo, bloqueado: !contato.bloqueado })
      .then(() => Promise.all([desenharAgenda(), desenharAmigos()]))
      .catch((falha) => mostrarErroDaAgenda(falha));
  });

  // **Só o clique faz amigo.** Perdida, desconhecida, ou de volta: nenhuma
  // ligação adiciona sozinha.
  let amigo;
  if (contato.amigo) {
    amigo = document.createElement("span");
    amigo.className = "perfil-agenda-amigo";
    amigo.textContent = SALA_PESSOAL.amigos.jaAmigo;
  } else {
    amigo = document.createElement("button");
    amigo.type = "button";
    amigo.className = "botao-fantasma";
    amigo.textContent = SALA_PESSOAL.amigos.adicionarAosAmigos;
    amigo.addEventListener("click", () => {
      invoke("adicionar_amigo", { codigo: contato.codigo, nome: contato.nome ?? null })
        .then(() => Promise.all([desenharAgenda(), desenharAmigos()]))
        .catch((falha) => mostrarErroDaAgenda(falha));
    });
  }

  linha.append(nome, quem, amigo, bloquear);
  return linha;
}

function mostrarErroDaAgenda(falha) {
  const erro = $("perfil-agenda-erro");
  erro.textContent = SALA_PESSOAL.naAgenda[falha] ?? SALA_PESSOAL.naAgenda.NaoGravou;
  erro.hidden = false;
}

/** A agenda no PERFIL. */
async function desenharAgenda() {
  $("perfil-agenda-erro").hidden = true;
  let contatos = [];
  try {
    contatos = (await invoke("contatos_da_agenda")) ?? [];
  } catch (falha) {
    mostrarErroDaAgenda(falha);
  }
  $("perfil-agenda").replaceChildren(...contatos.map(linhaDaAgenda));
  $("perfil-agenda-vazia").hidden = contatos.length > 0;
}

// ------------------------------------------------------------------ amigos

function mostrarErroDosAmigos(falha) {
  const erro = $("boot-amigos-erro");
  erro.textContent = SALA_PESSOAL.naAgenda[falha] ?? SALA_PESSOAL.naAgenda.NaoGravou;
  erro.hidden = false;
}

/** As iniciais de quem, para o ladrilho do cartão (nome ou código). */
function iniciaisDe(quem) {
  const letras = String(quem).match(/[\p{L}\p{N}]/gu) ?? ["?"];
  return letras.slice(0, 2).join("").toUpperCase();
}

/**
 * Um cartão da lista de amigos: o nome em destaque (e renomeável), o código
 * menor, o LIGAR como ação principal, e o TIRAR DOS AMIGOS discreto.
 */
function linhaDoAmigo(contato) {
  const quem = contato.nome || contato.codigo;
  const linha = document.createElement("li");
  linha.className = "amigo-cartao";

  const avatar = document.createElement("span");
  avatar.className = "amigo-avatar";
  avatar.setAttribute("aria-hidden", "true");
  avatar.textContent = iniciaisDe(quem);

  const nome = document.createElement("input");
  nome.className = "amigo-nome";
  nome.value = contato.nome ?? "";
  nome.maxLength = 48;
  nome.spellcheck = false;
  nome.placeholder = "dar um nome";
  nome.title = "clique para renomear";
  nome.setAttribute("aria-label", `Nome para ${contato.codigo}`);
  nome.addEventListener("change", () => {
    invoke("dar_nome_na_agenda", { codigo: contato.codigo, nome: nome.value.trim() })
      .then(() => desenharAmigos())
      .catch((falha) => mostrarErroDosAmigos(falha));
  });

  const detalhe = document.createElement("div");
  detalhe.className = "amigo-detalhe";
  const codigo = document.createElement("span");
  codigo.className = "amigo-codigo";
  codigo.textContent = contato.codigo;
  detalhe.append(codigo);
  if (contato.bloqueado) {
    const aviso = document.createElement("span");
    aviso.className = "amigo-aviso";
    aviso.textContent = SALA_PESSOAL.amigos.bloqueado;
    detalhe.append(aviso);
  }

  const texto = document.createElement("div");
  texto.className = "amigo-texto";
  texto.append(nome, detalhe);

  // LIGAR é o caminho do código colado, sem o campo: abre MAIS UMA sessão e
  // não derruba as outras nem o servidor hospedado. O `alvo` vem montado do Rust.
  const ligar = document.createElement("button");
  ligar.type = "button";
  ligar.className = "amigo-ligar";
  ligar.textContent = SALA_PESSOAL.amigos.ligar;
  ligar.disabled = contato.bloqueado === true;
  ligar.setAttribute("aria-label", SALA_PESSOAL.amigos.ligarA(quem));
  ligar.addEventListener("click", () => {
    irParaOServidor(contato.alvo, null, null, quem, null, true);
  });

  const tirar = document.createElement("button");
  tirar.type = "button";
  tirar.className = "amigo-tirar";
  tirar.textContent = SALA_PESSOAL.amigos.tirar;
  tirar.title = SALA_PESSOAL.amigos.tirarDe(quem);
  tirar.setAttribute("aria-label", SALA_PESSOAL.amigos.tirarDe(quem));
  tirar.addEventListener("click", () => {
    invoke("tirar_dos_amigos", { codigo: contato.codigo })
      .then(() => Promise.all([desenharAmigos(), desenharAgenda()]))
      .catch((falha) => mostrarErroDosAmigos(falha));
  });

  const acoes = document.createElement("div");
  acoes.className = "amigo-acoes";
  acoes.append(ligar, tirar);
  linha.append(avatar, texto, acoes);
  return linha;
}

/** A lista de AMIGOS da entrada; escondida onde a agenda não existe. */
async function desenharAmigos() {
  $("boot-amigos-erro").hidden = true;
  $("boot-amigos-tentar").hidden = true;
  let amigos;
  let falhou = null;
  try {
    amigos = await invoke("amigos");
  } catch (falha) {
    amigos = null;
    falhou = falha;
  }
  const bloco = $("boot-amigos");
  // **Só a recusa esperada do navegador esconde o bloco**: ali a agenda não
  // existe. Uma falha do desktop (NaoGravou, ou outra) aparece, com a frase e o
  // TENTAR DE NOVO: o recurso existe, e sumir com ele seria calar o defeito.
  if (!Array.isArray(amigos) && falhou?.RecusaDaPonte) {
    bloco.hidden = true;
    return;
  }
  if (!Array.isArray(amigos)) {
    $("boot-amigos-lista").replaceChildren();
    $("boot-amigos-vazia").hidden = true;
    $("boot-amigos-nota").hidden = true;
    mostrarErroDosAmigos(falhou);
    $("boot-amigos-tentar").hidden = false;
    bloco.hidden = false;
    return;
  }
  $("boot-amigos-lista").replaceChildren(...amigos.map(linhaDoAmigo));
  $("boot-amigos-vazia").textContent = SALA_PESSOAL.amigos.vazia;
  $("boot-amigos-vazia").hidden = amigos.length > 0;
  $("boot-amigos-nota").textContent = amigos.length > 0 ? SALA_PESSOAL.amigos.tirarDiz : "";
  $("boot-amigos-detalhe").hidden = amigos.length === 0;
  bloco.hidden = false;
}

$("boot-amigos-form").addEventListener("submit", (evento) => {
  evento.preventDefault();
  const codigo = $("boot-amigo-codigo").value.trim();
  const nome = $("boot-amigo-nome").value.trim();
  if (!codigo) return;
  invoke("adicionar_amigo", { codigo, nome: nome || null })
    .then(() => {
      $("boot-amigo-codigo").value = "";
      $("boot-amigo-nome").value = "";
      return desenharAmigos();
    })
    .catch((falha) => mostrarErroDosAmigos(falha));
});

$("boot-amigos-tentar").addEventListener("click", () => {
  desenharAmigos().catch((falha) => console.warn("amigos:", falha));
});

new MutationObserver(() => {
  if (!$("tela-boot").hidden) desenharAmigos().catch((falha) => console.warn("amigos:", falha));
}).observe($("tela-boot"), { attributes: true, attributeFilter: ["hidden"] });
desenharAmigos().catch((falha) => console.warn("amigos:", falha));

/** O bloco da sala no PERFIL, quando o PERFIL abre. */
async function desenharSalaNoPerfil() {
  const bloco = $("perfil-sala");
  let codigo = null;
  try {
    codigo = await invoke("meu_codigo");
  } catch {
    codigo = null;
  }
  if (!codigo) {
    bloco.hidden = true;
    return;
  }
  $("perfil-codigo").value = codigo;
  $("perfil-copiar-codigo").textContent = "COPIAR";
  desenharEstadoDaSala();
  await desenharAgenda();
  bloco.hidden = false;
}

// O PERFIL abre por mais de um caminho (a entrada, o operador, a pergunta do
// nome), e quem o abre é o `camada-nomear.js`. O bloco da sala se redesenha
// quando o diálogo aparece, sem que aquele arquivo precise saber dele.
new MutationObserver(() => {
  if (!$("perfil").hidden) {
    desenharSalaNoPerfil().catch((falha) => console.warn("a sala no perfil:", falha));
  }
}).observe($("perfil"), { attributes: true, attributeFilter: ["hidden"] });

$("perfil-copiar-codigo").addEventListener("click", () => {
  copiarLink($("perfil-codigo"), $("perfil-copiar-codigo")).catch((falha) =>
    console.warn("copiar o código:", falha),
  );
});

$("perfil-receber").addEventListener("change", () => {
  const receber = $("perfil-receber").checked;
  invoke("receber_ligacoes", { receber }).catch((falha) =>
    console.warn("receber_ligacoes:", falha),
  );
});

/** O SEU CÓDIGO na entrada: independente de sessão, escondido sem resposta. */
async function desenharCodigoNaEntrada() {
  let codigo = null;
  try {
    codigo = await invoke("meu_codigo");
  } catch {
    codigo = null;
  }
  $("boot-codigo-valor").value = codigo ?? "";
  $("boot-codigo").hidden = !codigo;
  if (codigo) $("boot-codigo-estado").textContent = fraseDoCodigo(estadoDaSala);
}

$("boot-copiar-codigo").addEventListener("click", () => {
  copiarLink($("boot-codigo-valor"), $("boot-copiar-codigo")).catch((falha) =>
    console.warn("copiar o código:", falha),
  );
});

$("perfil-sala-firewall-abrir").addEventListener("click", async () => {
  const abrir = $("perfil-sala-firewall-abrir");
  abrir.disabled = true;
  try {
    await invoke("abrir_a_porta_no_firewall");
    $("perfil-sala-firewall-frase").textContent =
      "Regra criada. Quem liga pelo seu código de fora da sua rede passa a chegar.";
    regraDoFirewallCriada = true;
    abrir.hidden = true;
  } catch (falha) {
    $("perfil-sala-firewall-frase").textContent = fraseDeErro(falha);
  } finally {
    abrir.disabled = false;
  }
});

listen("seele://sala", ({ payload }) => {
  estadoDaSala = payload;
  // A sala que desce e sobe de novo traz uma leitura nova do firewall.
  if (payload?.estado !== "NoAr") regraDoFirewallCriada = false;
  desenharEstadoDaSala();
}).catch((falha) => console.warn("seele://sala:", falha));

// A tela que abre depois do primeiro anúncio. Um anúncio que chegue antes da
// resposta vale mais que ela, por ser mais novo.
invoke("estado_da_sala")
  .then((estado) => {
    estadoDaSala ??= estado;
    desenharEstadoDaSala();
  })
  .catch(() => {})
  .finally(() => desenharCodigoNaEntrada());
