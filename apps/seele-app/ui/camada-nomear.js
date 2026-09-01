// Nomear: um diálogo para dar nome a qualquer coisa.
//
// # Por que um só, e não um por caso
//
// Três coisas passam por aqui — criar sala de voz, criar canal, renomear sala —
// e as três pedem exatamente o mesmo de quem usa: um nome curto, um Enter, e um
// motivo por escrito quando o servidor recusa. O que muda entre elas são
// palavras, e palavras não justificam três marcações, três armadilhas de foco e
// três lugares para o `maxlength` divergir.
//
// # O que ele substitui
//
// Os dois formulários `criar` que moravam **abertos** na coluna da esquerda. A
// comp da 0.9.0 os troca por um `+` no cabeçalho de cada lista, e o ganho não é
// de espaço: um formulário sempre visível numa coluna de navegação é um convite
// a preencher, e criar canal não é o que se faz ali toda hora.
//
// # O que este arquivo não decide
//
// O que acontece com o nome. Quem abre passa um `aoConfirmar`, e é dele o
// comando, a recusa e o que fazer depois. Este arquivo sabe abrir, fechar,
// devolver o foco e escrever o motivo — e mais nada.

"use strict";

/** Para onde o teclado volta quando o diálogo fecha. */
let focoAntesDeNomear = null;

/** O que fazer com o nome, posto por quem abriu. */
let aoConfirmarNome = null;

/**
 * Abre o diálogo.
 *
 * @param {object} pedido
 * @param {string} pedido.titulo      o cabeçalho, em caixa alta
 * @param {string} pedido.rotulo      o que se está nomeando
 * @param {string} pedido.exemplo     o `placeholder`
 * @param {string} pedido.acao        o verbo do botão que confirma
 * @param {string} [pedido.valor]     o nome de agora, ao renomear
 * @param {(nome: string) => Promise<void>} pedido.aoConfirmar
 */
function abrirNomear(pedido) {
  focoAntesDeNomear = document.activeElement;
  aoConfirmarNome = pedido.aoConfirmar;

  $("nomear-titulo").textContent = pedido.titulo;
  $("nomear-rotulo").textContent = pedido.rotulo;
  $("nomear-confirmar").textContent = pedido.acao;
  $("nomear-erro").hidden = true;
  $("nomear-erro").textContent = "";

  const campo = $("nomear-valor");
  campo.placeholder = pedido.exemplo ?? "";
  campo.value = pedido.valor ?? "";

  $("nomear").hidden = false;
  campo.focus();
  // Selecionado, e não com o cursor no fim: ao renomear, o gesto seguinte é
  // quase sempre trocar o nome inteiro, e quem só quiser emendar aperta uma
  // seta. O contrário obrigaria todo mundo a apagar antes de escrever.
  campo.select();
  anunciar(`${pedido.titulo}. Escape fecha.`);
}

/**
 * Fecha, devolvendo o teclado a quem abriu.
 *
 * `focavel` antes do `focus()` pela mesma razão da ajuda: a lista de baixo pode
 * ter sido redesenhada enquanto o diálogo estava aberto — e um `focus()` num nó
 * que saiu do documento não faz nada e não avisa.
 */
function fecharNomear() {
  $("nomear").hidden = true;
  aoConfirmarNome = null;
  if (focavel(focoAntesDeNomear)) {
    focoAntesDeNomear.focus();
  }
  focoAntesDeNomear = null;
}

/** Diz, onde se acabou de digitar, por que o nome não passou. */
function recusarNome(frase) {
  const onde = $("nomear-erro");
  // Revelar antes de escrever: `role="alert"` não anuncia o que já estava na
  // página enquanto ela estava escondida.
  onde.hidden = false;
  onde.textContent = frase;
  $("nomear-valor").focus();
}

$("nomear-forma").addEventListener("submit", async (evento) => {
  evento.preventDefault();
  const nome = $("nomear-valor").value.trim();
  // Vazio não é pedido: é o Enter de quem ainda não escreveu. O campo continua
  // aberto e nada é mandado.
  if (nome === "") return;
  if (!aoConfirmarNome) return;

  const confirmar = $("nomear-confirmar");
  confirmar.disabled = true;
  try {
    await aoConfirmarNome(nome);
    fecharNomear();
  } catch (falha) {
    console.warn("nomear:", falha);
    recusarNome(fraseDeErro(falha));
  } finally {
    confirmar.disabled = false;
  }
});

$("nomear-cancelar").addEventListener("click", fecharNomear);
fecharAoClicarFora("nomear", fecharNomear);

// `Escape` fecha, e em fase de captura: com o diálogo aberto ele é a coisa mais
// de cima da janela, e a tecla não pode fechar a busca ou a gaveta atrás dele.
window.addEventListener(
  "keydown",
  (evento) => {
    if (evento.key !== "Escape" || $("nomear").hidden) return;
    evento.preventDefault();
    evento.stopPropagation();
    fecharNomear();
  },
  true,
);

// --------------------------------------------------------------- seu perfil
//
// A imagem e o apelido. Mora aqui, junto do diálogo de nomear, porque é o
// mesmo tipo de coisa — uma camada que pergunta uma coisa e fecha — e porque
// as duas dividem o desenho da caixa.

/** Para onde o teclado volta quando o perfil fecha. */
let focoAntesDoPerfil = null;

/** Desenha a prévia: a imagem, ou a inicial que os outros veem sem ela. */
async function desenharPrevia(pessoa, apelido) {
  const onde = $("perfil-previa");
  let imagem = null;
  try {
    imagem = await invoke("imagem_da_pessoa", { person: pessoa });
  } catch (falha) {
    console.warn("imagem_da_pessoa:", falha);
  }
  onde.replaceChildren();
  if (imagem) {
    const figura = document.createElement("img");
    figura.src = imagem;
    figura.alt = "";
    onde.append(figura);
    onde.setAttribute("aria-label", `imagem de ${apelido}`);
    onde.dataset.tem = "sim";
  } else {
    // A inicial, que é o que a lista de pessoas desenha para quem não pôs
    // imagem: a prévia mostra o que os outros veem, e não um vazio.
    onde.textContent = (apelido || "?").trim().charAt(0).toUpperCase();
    onde.setAttribute("aria-label", `${apelido} não tem imagem`);
    onde.dataset.tem = "nao";
  }
}

/** Abre o perfil de quem está usando esta janela. */
async function abrirPerfil() {
  focoAntesDoPerfil = document.activeElement;
  $("perfil-erro").hidden = true;
  let snapshot = null;
  try {
    snapshot = await invoke("snapshot");
  } catch (falha) {
    console.warn("snapshot:", falha);
  }
  const apelido = snapshot?.nickname ?? "—";
  $("perfil-apelido").textContent = apelido;
  await desenharPrevia(snapshot?.me ?? 0, apelido);
  $("perfil").hidden = false;
  $("perfil-escolher").focus();
  anunciar("Seu perfil. Escape fecha.");
}

/** Fecha, devolvendo o teclado. */
function fecharPerfil() {
  $("perfil").hidden = true;
  if (focavel(focoAntesDoPerfil)) focoAntesDoPerfil.focus();
  focoAntesDoPerfil = null;
}

/** Diz por que a imagem não entrou, onde se acabou de tentar. */
function recusarPerfil(frase) {
  const onde = $("perfil-erro");
  onde.hidden = false;
  onde.textContent = frase;
}

$("perfil-escolher").addEventListener("click", async () => {
  $("perfil-erro").hidden = true;
  try {
    // `false` é ter fechado o seletor sem escolher, e é o desfecho mais comum
    // de todos — não é falha e não escreve nada.
    if (await invoke("escolher_minha_imagem")) {
      anunciar("Imagem trocada.");
      await abrirPerfil();
    }
  } catch (falha) {
    console.warn("escolher_minha_imagem:", falha);
    recusarPerfil(fraseDeErro(falha));
  }
});

$("perfil-tirar").addEventListener("click", async () => {
  $("perfil-erro").hidden = true;
  try {
    await invoke("tirar_minha_imagem");
    anunciar("Imagem tirada.");
    await abrirPerfil();
  } catch (falha) {
    console.warn("tirar_minha_imagem:", falha);
    recusarPerfil(fraseDeErro(falha));
  }
});

// O formulário não tem botão de mandar — ver a nota no `index.html`. O ouvinte
// de `submit` fica porque um Enter dentro de um `<form>` ainda o dispara, e sem
// ele o navegador recarregaria a página: exatamente o que a barra de janela
// desta versão foi ao trabalho de impedir.
$("perfil-forma").addEventListener("submit", (evento) => {
  evento.preventDefault();
  fecharPerfil();
});
$("perfil-fechar").addEventListener("click", fecharPerfil);
$("operador-quem").addEventListener("click", () => {
  abrirPerfil().catch((falha) => console.warn("abrir o perfil:", falha));
});
fecharAoClicarFora("perfil", fecharPerfil);

window.addEventListener(
  "keydown",
  (evento) => {
    if (evento.key !== "Escape" || $("perfil").hidden) return;
    evento.preventDefault();
    evento.stopPropagation();
    fecharPerfil();
  },
  true,
);
