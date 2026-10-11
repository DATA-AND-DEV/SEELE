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
    // **O teto do fio, em bytes, antes de mandar** (pendência 58, item 55;
    // lote C55 da F5). Tudo o que passa por aqui é nome de canal ou de sala, e
    // os dois têm o mesmo teto. Era um `maxlength="48"` no campo, que contava
    // caracteres: um nome acentuado cabia nele e não no quadro, e o núcleo o
    // recusava sem a janela saber. A recusa fica aqui, onde se escreveu.
    const grandeDemais = await passaDoTeto("nome_de_canal", nome);
    if (grandeDemais) {
      recusarNome(grandeDemais);
      return;
    }
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
async function desenharPrevia(apelido) {
  const onde = $("perfil-previa");
  // **`meu_retrato` e não `imagem_da_pessoa`**, e é o que faz os dois diálogos
  // serem o mesmo: o retrato é seu e mora nesta máquina, então a prévia tem
  // resposta com sessão e sem. Perguntar ao servidor pela própria imagem só
  // funcionava dentro dele — era por isso que a tela inicial escondia o bloco
  // inteiro e mostrava um diálogo diferente com o mesmo nome.
  let imagem = null;
  try {
    imagem = await invoke("meu_retrato");
  } catch (falha) {
    console.warn("meu_retrato:", falha);
  }
  $("perfil-tirar").disabled = !imagem;
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

/**
 * Abre o perfil de quem está usando esta janela.
 *
 * `titulo` é opcional, e só a pergunta do nome antes da primeira entrada o
 * passa (`perguntarONome`). Sem ele, o diálogo é o SEU PERFIL de sempre.
 *
 * O diálogo se reabre de dentro — trocar a imagem, uma recusa ao gravar o
 * apelido —, e essas reaberturas não trazem título. Então um título só volta a
 * SEU PERFIL quando o diálogo está fechado: no meio da pergunta, reabrir para
 * mostrar a imagem nova não pode trocá-la por outra.
 */
async function abrirPerfil(titulo) {
  if (titulo !== undefined) {
    $("perfil-titulo").textContent = titulo;
  } else if ($("perfil").hidden) {
    $("perfil-titulo").textContent = "SEU PERFIL";
    perguntandoONome = false;
  }
  focoAntesDoPerfil = document.activeElement;
  $("perfil-erro").hidden = true;

  // **Dois modos, e a diferença é haver servidor.**
  //
  // Dentro de uma sessão, o perfil é o daquele servidor: o apelido vem do
  // `snapshot` e a troca é um comando de protocolo, porque o nome é único lá e
  // o roster de todo mundo tem de saber.
  //
  // Antes de conectar não há servidor nem sessão — e a tela de entrada da 0.9.0
  // abre o perfil justamente aí, porque tirou o campo de apelido de si mesma.
  // O que se edita então é a preferência desta máquina: o nome com que se entra
  // quando não se diz outro.
  let snapshot = null;
  try {
    snapshot = await invoke("snapshot");
  } catch (falha) {
    // `NotConnected` não é falha: é a tela de entrada.
    if (falha !== "NotConnected") console.warn("snapshot:", falha);
  }

  let apelido = snapshot?.nickname ?? "";
  if (!snapshot) {
    try {
      apelido = (await invoke("apelido_local")) ?? "";
    } catch (falha) {
      console.warn("apelido_local:", falha);
    }
  }
  $("perfil-apelido").value = apelido;
  // **A capa de quem usa no cabeçalho** (D-capa-1, do dono; rascunho da API 6,
  // §4, «o SEU PERFIL nativo, por um aplicador do produto»): dentro de uma
  // sessão, a do `me`; antes de conectar não há pessoa, e a camada sai. O
  // título e o FECHAR ficam sobre placas (`camada-nomear.css`).
  if (typeof vestirCapa === "function") {
    vestirCapa(document.querySelector("#perfil .perfil-cabeca"), snapshot?.me ?? null);
  }
  // Antes de a caixa aparecer e de o foco chegar ao campo: o leitor de tela lê a
  // descrição do campo nesse instante, e ela tem de estar escrita.
  desenharNotasDoApelido();

  // **O bloco da imagem aparece sempre**, e é o que faz os dois diálogos serem
  // um só.
  //
  // Ele ficava escondido sem sessão, porque o retrato só existia do lado do
  // servidor — e o diálogo da tela inicial virava outro diálogo, com o mesmo
  // título e metade do conteúdo. O relato de campo foi exatamente esse: «o
  // modal de perfil da tela inicial é diferente do modal de perfil dentro do
  // server».
  //
  // Hoje o retrato mora nesta máquina, como o apelido, e sobe junto com a
  // conexão. Escolher uma imagem antes de entrar em qualquer lugar passou a
  // ter consequência.
  $("perfil-imagem").hidden = false;
  await desenharPrevia(apelido);
  $("perfil").hidden = false;
  $("perfil-escolher").focus();
  // O título que está escrito, e não um fixo: na pergunta do nome, quem não vê
  // a tela precisa ouvir a pergunta, e não «seu perfil».
  const cabeca = $("perfil-titulo").textContent;
  anunciar(cabeca === "SEU PERFIL" ? "Seu perfil. Escape fecha." : `${cabeca} Escape fecha.`);
}

/**
 * Quem espera o perfil fechar, ou `null`.
 *
 * Só a pergunta do nome espera: a entrada não segue para o `connect` enquanto
 * a caixa está aberta. `fecharPerfil` é a única porta de saída do diálogo —
 * SALVAR, CANCELAR, FECHAR, Escape e o clique fora passam todos por ela —, e é
 * ela que solta quem espera.
 */
let aoFecharOPerfil = null;

/** Se a caixa aberta é a pergunta do nome, e não o SEU PERFIL de sempre. */
let perguntandoONome = false;

/**
 * As duas notas embaixo do campo do apelido, conforme a caixa e o campo.
 *
 * - **A de quem fecha sem nome** (`ENTRAR_SEM_NOME`) só aparece na pergunta, e
 *   só com o campo vazio: embaixo de um nome já digitado, «você vai entrar com
 *   um nome tirado da sua chave» contradiz o que está escrito logo acima. Quem
 *   digita e fecha sem salvar ainda entra com o nome da chave — e ouve isso ao
 *   fechar (`perguntarONome`).
 * - **A de sempre** («O que você já disse continua com o nome de então») fala
 *   com quem troca de nome. Na pergunta ninguém disse nada ainda, e ela sai.
 *
 * A de quem fecha sem nome fica vazia quando escondida, e não só escondida: o
 * campo a tem em `aria-describedby`, e a descrição lê até o texto de um nó
 * escondido.
 */
function desenharNotasDoApelido() {
  const vazio = $("perfil-apelido").value.trim() === "";
  const nota = $("perfil-sem-nome");
  const mostrar = perguntandoONome && vazio;
  nota.hidden = !mostrar;
  nota.textContent = mostrar ? ENTRAR_SEM_NOME : "";
  $("perfil-nota-do-apelido").hidden = perguntandoONome;
}

$("perfil-apelido").addEventListener("input", desenharNotasDoApelido);

/**
 * Pergunta o nome antes da primeira entrada, e devolve o que ficou gravado.
 *
 * É o perfil, com outro título. SALVAR grava com `escolher_apelido_local`,
 * como sempre fora de sessão (`guardarOApelido`), e o nome é relido depois de a
 * caixa fechar: a resposta é o que está no disco, e não o que estava no campo —
 * fechar com o campo preenchido e sem salvar não grava nada.
 *
 * Fechar sem nome segue como antes, para o nome tirado da chave, e diz isso: a
 * nota aparece na caixa enquanto o campo está vazio, que é quando ainda dá para
 * mudar de ideia — e o leitor de tela a ouve ao chegar ao campo, pela
 * descrição dele —, e é anunciada ao fechar. Ver `desenharNotasDoApelido`.
 */
async function perguntarONome() {
  const fechou = new Promise((resolver) => {
    aoFecharOPerfil = resolver;
  });
  // Antes de abrir: é `abrirPerfil` que desenha as notas, com o campo já cheio.
  perguntandoONome = true;
  try {
    await abrirPerfil("COMO OS OUTROS VÃO TE CHAMAR?");
    // O teclado no nome, e não na imagem, onde o perfil de sempre o põe: a
    // pergunta é o nome, e escrever e apertar Enter é a resposta inteira.
    $("perfil-apelido").focus();
  } catch (falha) {
    // Uma caixa que não abriu não pode prender a entrada atrás dela.
    console.warn("abrir o perfil:", falha);
    fecharPerfil();
  }
  await fechou;
  let apelido = "";
  try {
    apelido = ((await invoke("apelido_local")) ?? "").trim();
  } catch (falha) {
    console.warn("apelido_local:", falha);
  }
  if (apelido === "") anunciar(ENTRAR_SEM_NOME);
  return apelido;
}

/** Fecha, devolvendo o teclado. */
function fecharPerfil() {
  $("perfil").hidden = true;
  // As notas da pergunta do nome são da pergunta, e ela acabou.
  perguntandoONome = false;
  desenharNotasDoApelido();
  // Quem esperava a caixa fechar segue — ver `aoFecharOPerfil`.
  const seguir = aoFecharOPerfil;
  aoFecharOPerfil = null;
  if (seguir) seguir();
  // O rodapé da entrada mostra o nome escolhido, e ele pode ter acabado de
  // mudar. Redesenhar aqui é o que faz a caixa e o rodapé nunca discordarem.
  if (typeof desenharPerfilDaEntrada === "function") {
    desenharPerfilDaEntrada().catch(() => {});
  }
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
/**
 * Guarda o apelido, e diz quem o guardou.
 *
 * **`naSessao` vinha da tela, e a tela mentiu.** Aqui estava
 * `!$("perfil-imagem").hidden` — o bloco da imagem só aparecia dentro de um
 * servidor, então a visibilidade dele servia de resposta. No dia em que o bloco
 * passou a aparecer sempre, para os dois diálogos serem o mesmo, a resposta
 * virou «sim» em todo lugar: a tela inicial passou a chamar o comando de
 * sessão, que não tem sessão a que obedecer, e o nome não era gravado em canto
 * nenhum.
 *
 * A pergunta agora é feita a quem sabe respondê-la. Uma tela lida como estado é
 * um estado que muda quando alguém mexe no desenho.
 */
async function guardarOApelido() {
  const escolhido = $("perfil-apelido").value.trim();
  if (escolhido === "") {
    await abrirPerfil();
    return false;
  }
  $("perfil-erro").hidden = true;

  // **O teto do fio, em bytes, antes de mandar e antes de gravar** (pendência
  // 58, item 55; lote C55 da F5). As duas portas têm o mesmo teto: o
  // `SetNickname` da sessão e o apelido gravado nesta máquina, que vai no
  // `Hello` da conexão seguinte. O gravado grande demais fazia o `connect`
  // voltar `Unreachable`, e a pessoa lia NADA RESPONDEU NESSE ENDEREÇO com o
  // servidor de pé. A caixa fica aberta, com o que se escreveu, para encurtar.
  const grandeDemais = await passaDoTeto("apelido", escolhido);
  if (grandeDemais) {
    recusarPerfil(grandeDemais);
    $("perfil-apelido").focus();
    return false;
  }

  let naSessao = false;
  try {
    naSessao = (await invoke("snapshot")) !== null;
  } catch (falha) {
    // `NotConnected` é a resposta, e não uma falha: quem não está num servidor
    // grava nesta máquina.
    if (falha !== "NotConnected") console.warn("snapshot:", falha);
  }

  try {
    if (naSessao) {
      await invoke("escolher_apelido", { apelido: escolhido });
    } else {
      await invoke("escolher_apelido_local", { apelido: escolhido });
    }
    anunciar(`Agora você é ${escolhido}.`);
    return true;
  } catch (falha) {
    console.warn("escolher_apelido:", falha);
    recusarPerfil(fraseDeErro(falha));
    await abrirPerfil();
    return false;
  }
}

// `SALVAR` é o `submit` da forma, então o Enter dentro do campo continua
// valendo — são a mesma porta, e não duas.
$("perfil-forma").addEventListener("submit", (evento) => {
  evento.preventDefault();
  guardarOApelido()
    .then((deuCerto) => {
      if (deuCerto) fecharPerfil();
    })
    .catch((falha) => console.warn("guardar o apelido:", falha));
});

// `CANCELAR` fecha sem gravar o campo. A imagem já foi aplicada quando se
// apertou `ESCOLHER`, e este botão não promete desfazê-la: ele é do apelido,
// que é a única coisa aqui que espera para valer.
$("perfil-cancelar").addEventListener("click", fecharPerfil);

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
