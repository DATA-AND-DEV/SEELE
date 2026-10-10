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

/** Escreve a frase da sala onde a pessoa está, ou esconde. */
function desenharEstadoDaSala() {
  const frase = fraseDaSala(estadoDaSala);
  for (const id of ["boot-sala-estado", "sala-estado", "perfil-sala-estado"]) {
    const linha = $(id);
    linha.textContent = frase ?? "";
    linha.hidden = !frase;
  }
  // «Receber ligações» está marcado em todo estado menos o desligado: a sala
  // subindo, no ar ou que falhou ao subir são todas «quero receber».
  $("perfil-receber").checked = estadoDaSala?.estado !== "Desligada";

  // O firewall do Windows (D-sala-1): a frase do Rust, o que a 8385 é, e o
  // botão que pede a regra ao Windows.
  const firewall = estadoDaSala?.estado === "NoAr" ? estadoDaSala.firewall_nao_cobre : null;
  $("perfil-sala-firewall-frase").textContent = firewall
    ? `${firewall} ${SALA_PESSOAL.firewall}`
    : "";
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
      .then(() => desenharAgenda())
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
      .then(() => desenharAgenda())
      .catch((falha) => mostrarErroDaAgenda(falha));
  });

  linha.append(nome, quem, bloquear);
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

$("perfil-sala-firewall-abrir").addEventListener("click", async () => {
  const abrir = $("perfil-sala-firewall-abrir");
  abrir.disabled = true;
  try {
    await invoke("abrir_a_porta_no_firewall");
    $("perfil-sala-firewall-frase").textContent =
      "Regra criada. Quem liga pelo seu código de fora da sua rede passa a chegar.";
    abrir.hidden = true;
  } catch (falha) {
    $("perfil-sala-firewall-frase").textContent = fraseDeErro(falha);
  } finally {
    abrir.disabled = false;
  }
});

listen("seele://sala", ({ payload }) => {
  estadoDaSala = payload;
  desenharEstadoDaSala();
}).catch((falha) => console.warn("seele://sala:", falha));

// A tela que abre depois do primeiro anúncio. Um anúncio que chegue antes da
// resposta vale mais que ela, por ser mais novo.
invoke("estado_da_sala")
  .then((estado) => {
    estadoDaSala ??= estado;
    desenharEstadoDaSala();
  })
  .catch(() => {});
