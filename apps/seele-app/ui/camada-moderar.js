// SEELE · Entry Plug — a moderação (`#moderar`), camada sobre a operação.
//
// Os quatro verbos que a `specs/04-servidor-seele.md` dá ao Comandante e ao
// Operador: expulsar, banir, remover mensagem e mover piloto. O Rust os tinha
// prontos e nada os chamava — o `EJETAR PLUG DO OPERADOR` do comp v2 está
// desenhado e desabilitado desde então, com um `title` dizendo que não havia o
// que chamar.
//
// ---- por que uma camada, e não uma tela ----
//
// Pela mesma razão que o alerta e a bateria são camadas: `specs/07` não deixa
// esta janela trocar a conversa por outra coisa, e moderar é uma decisão que se
// toma **olhando** para a sala. Uma tela cheia apagaria quem está falando no
// instante em que se decide o que fazer com alguém.
//
// ---- por que uma caixa, e não um botão na linha ----
//
// Expulsar e banir são irreversíveis **para quem sofre**, e banir não tem
// desfazer em lugar nenhum deste produto: `unban` não é verbo do protocolo, e
// um banimento hoje só se desfaz por quem tem acesso ao arquivo do Dogma, à
// mão, no computador que o hospeda. Isso decide a forma: é preciso uma
// superfície larga o bastante para escrever a **consequência** antes de o ato
// sair, com o nome de quem vai sofrê-la dentro da frase.
//
// «Tem certeza?» não é confirmação: é uma pergunta que não acrescenta
// informação nenhuma a quem já apertou uma vez. Dizer o que vai acontecer é.
// Daí `armarAto`, por onde passam os quatro verbos e mais nada.
//
// ---- o que este arquivo não decide ----
//
// Se a pessoa pode. Os quatro booleanos do `Snapshot` (`may_kick`, `may_ban`,
// `may_remove_message`, `may_move_pilot`) dizem o que **oferecer**, e são
// quatro porque a `specs/04` enumera quatro permissões e um papel pode ter
// qualquer subconjunto delas. Esconder aqui é conveniência; quem nega é o
// servidor, e a `specs/08-seguranca.md` põe a segurança nessa recusa e não no
// botão escondido. Apertar sem a permissão volta como `NoticeRaised` com
// `PermissionDenied`, pelo mesmo caminho de qualquer outro aviso.

"use strict";

/** O ato armado, esperando confirmação. `null` quando não há nenhum. */
let atoArmado = null;

/**
 * Para onde o foco volta quando a caixa fecha.
 *
 * Guardado como elemento e não como `id`, pela mesma razão que `focoDeVolta` em
 * `base.js`: metade dos controles que abrem esta caixa é desenhada pelo
 * JavaScript e não tem `id` nenhum — o botão de moderar de um piloto é
 * reconstruído a cada snapshot.
 */
let focoAntesDeModerar = null;

/**
 * Quem a porta nomeou, quando ela nomeou alguém. `null` quando se entrou pelo
 * alerta, que não sabe de quem está falando.
 */
let alvoPreferido = null;

/**
 * Se a caixa aberta tem uma escolha atrás da confirmação.
 *
 * Decide o que CANCELAR faz, e são coisas diferentes: cancelar um ato sobre uma
 * pessoa devolve à escolha de quem, e cancelar a remoção de uma mensagem fecha
 * — ali não há nada atrás, e voltar para um formulário de pilotos que ninguém
 * pediu seria a caixa inventando um passo.
 */
let caixaComEscolha = false;

/** Onde cada piloto está, por id, para as frases de consequência. */
const ondeEstaOPiloto = new Map();

/**
 * As salas do Dogma, como estavam quando a caixa abriu.
 *
 * Guardadas porque o destino de uma mudança depende de **quem** está escolhido:
 * a jaula em que a pessoa já está não é destino nenhum, e quem está escolhido
 * muda sem a caixa fechar. Uma cópia do instante da abertura e não uma leitura
 * a cada quadro — a lista por baixo é redesenhada duas vezes por segundo, e
 * refazer as opções nesse ritmo apagaria a escolha de quem está escolhendo.
 */
let salasDoDogma = [];

/** Quantos dias vale cada opção de banimento. Vazio é para sempre. */
const SEGUNDOS_POR_DIA = 86400;

/**
 * Há algum verbo de moderação sobre gente nesta sessão?
 *
 * Os três que agem sobre uma pessoa. `may_remove_message` fica de fora de
 * propósito: quem só pode apagar mensagem não tem o que fazer nesta caixa, e
 * abri-la seria oferecer três controles apagados.
 */
function podeModerarPilotos(snapshot) {
  return Boolean(
    snapshot && (snapshot.may_kick || snapshot.may_ban || snapshot.may_move_pilot),
  );
}

/**
 * O botão que abre a moderação de uma pessoa, ou `null` quando não há o que
 * oferecer.
 *
 * Mora na linha de quem está dentro de um Cage, que é o único lugar desta
 * janela que lista **todo mundo em toda sala**: o roster e a grade da chamada
 * mostram só o Cage ocupado, e moderar é justamente o que se faz com quem está
 * noutra sala.
 *
 * Nunca sobre si mesmo: expulsar-se é o `DESCONECTAR` do cabeçalho, banir-se
 * não é coisa que exista, e mover-se é o `ENTRAR NA JAULA` da própria lista.
 */
function botaoDeModerar(piloto, snapshot) {
  if (piloto.is_self || !podeModerarPilotos(snapshot)) return null;
  const botao = elemento("button", "moderar-porta", "MODERAR");
  botao.type = "button";
  botao.dataset.moderarPiloto = String(piloto.id);
  // O nome no rótulo acessível porque `MODERAR` sozinho se repete uma vez por
  // pessoa, e uma lista de botões idênticos é uma lista que não se navega.
  botao.setAttribute("aria-label", `moderar ${piloto.nickname}`);
  botao.title = `expulsar, banir ou mover ${piloto.nickname}`;
  return botao;
}

/**
 * O botão que tira uma mensagem da Linha, ou `null` quando não há o que
 * oferecer.
 *
 * Duas portas para o mesmo verbo, e a distinção é do servidor: apagar a
 * **própria** mensagem não pede permissão nenhuma, e a permissão da `specs/04`
 * diz «de outra pessoa». Por isso `mensagem.own` entra antes do booleano.
 */
function botaoDeRemoverMensagem(mensagem, pode) {
  if (!mensagem.own && !pode) return null;
  const botao = elemento("button", "moderar-remover", "remover");
  botao.type = "button";
  botao.dataset.moderarMensagem = String(mensagem.id);
  botao.setAttribute(
    "aria-label",
    `remover a mensagem de ${mensagem.author_nickname} das ${relogio(mensagem.at_seconds)}`,
  );
  return botao;
}

/**
 * O segundo botão da caixa de alerta, que o comp desenha e o v2 deixou morto.
 *
 * A ambiguidade do rótulo do comp (Q6 do inventário — «ejetar o plug **do**
 * operador» tanto pode ser o ato sobre outra pessoa quanto a saída ordenada por
 * quem está aqui) se resolveu para o primeiro lado: quem quer sair tem
 * DESCONECTAR no cabeçalho, escrito, e é o que faz o que diz.
 *
 * O que ele **não** pode fazer é agir sobre um sujeito que o alerta não tem: um
 * `Notice` diz a severidade e o motivo, e nunca de quem ele é — é a mesma
 * lacuna que deixa as três células desta caixa vazias. Então ele é a porta, e
 * não o ato: abre a caixa onde se escolhe quem, e o rótulo passou a dizer isso.
 *
 * Desabilitado na marcação e ligado daqui, e a ordem importa: antes do primeiro
 * snapshot esta janela não sabe que permissões tem, e um botão que nasce
 * apertável promete o que talvez não possa cumprir.
 */
function atualizarPortaDoAlerta(snapshot) {
  const porta = $("alerta-ejetar");
  const pode = podeModerarPilotos(snapshot);
  porta.disabled = !pode;
  porta.title = pode
    ? "escolher quem, e o que fazer: expulsar, banir ou mover"
    : "este Dogma não deu a você nenhuma permissão de moderação";
}

// ------------------------------------------------------------------- desenho

/**
 * Preenche a caixa a partir do snapshot: quem dá para moderar, para onde dá
 * para mover, e quais dos três verbos esta sessão pode usar.
 *
 * Escrito uma vez, ao abrir, e não a cada quadro. A tela por baixo redesenha
 * duas vezes por segundo; refazer as opções de um `<select>` nesse ritmo
 * apagaria a escolha de quem estava escolhendo.
 */
function desenharModeracao(snapshot) {
  ondeEstaOPiloto.clear();
  salasDoDogma = snapshot.cages;

  const quem = $("moderar-quem");
  const opcoes = [];
  for (const cage of snapshot.cages) {
    for (const piloto of cage.pilots) {
      if (piloto.is_self) continue;
      ondeEstaOPiloto.set(String(piloto.id), {
        nome: piloto.nickname,
        cage: cage.name,
        cageId: cage.id,
      });
      const opcao = elemento("option", null, `${piloto.nickname} — ${cage.name}`);
      opcao.value = String(piloto.id);
      opcoes.push(opcao);
    }
  }
  repovoar(quem, opcoes);

  // Ninguém a moderar não é falha nem permissão que falta: é um Dogma em que
  // todas as jaulas estão vazias, ou em que só você está dentro de uma. A frase
  // diz isso, e os três verbos somem — não ficam apagados, porque não há o que
  // explicar sobre um controle cujo objeto não existe.
  const ha = opcoes.length > 0;
  $("moderar-vazio").hidden = ha;
  $("moderar-sobre").hidden = !ha;
  if (ha) {
    // A porta que nomeou alguém pré-escolhe; a porta do alerta não nomeia
    // ninguém e cai na primeira pessoa da lista, que é o que um `<select>` faz.
    if (alvoPreferido && ondeEstaOPiloto.has(alvoPreferido)) quem.value = alvoPreferido;
  }

  // Um verbo por permissão, e não os três por uma. `specs/04` enumera quatro
  // permissões e um papel pode ter qualquer subconjunto: quem só expulsa não vê
  // banir, e quem só move não vê nenhum dos dois.
  $("moderar-acao-expulsar").hidden = !snapshot.may_kick;
  $("moderar-acao-banir").hidden = !snapshot.may_ban;
  // Mover some também quando não há para onde: um Dogma de uma jaula só não
  // tem destino, e um controle cujo objeto não existe é ruído, não lacuna.
  $("moderar-acao-mover").hidden = !snapshot.may_move_pilot || desenharDestinos() === 0;
}

/**
 * As jaulas para onde dá para mover quem está escolhido, e quantas são.
 *
 * A jaula em que a pessoa já está fica de fora: mover alguém para onde ela já
 * está não faz nada, e uma opção que não faz nada é uma opção que alguém vai
 * escolher — e depois vai perguntar por que não aconteceu nada.
 */
function desenharDestinos() {
  const escolhido = ondeEstaOPiloto.get($("moderar-quem").value);
  const destinos = salasDoDogma.filter((cage) => cage.id !== escolhido?.cageId);
  repovoar(
    $("moderar-para"),
    destinos.map((cage) => {
      const opcao = elemento("option", null, `${cage.name} — ${cage.pilots.length}/${cage.limit}`);
      opcao.value = String(cage.id);
      return opcao;
    }),
  );
  return destinos.length;
}

/** Quem está escolhido agora, com o nome e a sala em que está. */
function quemEstaEscolhido() {
  const id = $("moderar-quem").value;
  const onde = ondeEstaOPiloto.get(id);
  if (!onde) return null;
  return { id: Number(id), nome: onde.nome, cage: onde.cage };
}

// ------------------------------------------------------- armar e confirmar

/**
 * Põe um ato à espera de confirmação, com a consequência escrita.
 *
 * A frase é o argumento, e é obrigatória: um ato que chega aqui sem ela seria a
 * caixa perguntando «tem certeza?», que é a forma de confirmação que não
 * informa nada. Cada chamador escreve a sua, com o nome de quem sofre dentro.
 *
 * O foco pousa no CANCELAR de propósito. Para um ato que não se desfaz, a tecla
 * que já está debaixo do dedo tem que ser a que não faz nada.
 */
function armarAto(rotulo, consequencia, executar) {
  atoArmado = { rotulo, consequencia, executar };
  $("moderar-escolha").hidden = true;
  $("moderar-confirmacao").hidden = false;
  $("moderar-consequencia").textContent = consequencia;
  $("moderar-confirmar").textContent = rotulo;
  $("moderar-cancelar").focus();
}

/** Desarma sem ter mandado nada. */
function desarmarAto() {
  atoArmado = null;
  if (!caixaComEscolha) {
    fecharModeracao();
    return;
  }
  $("moderar-confirmacao").hidden = true;
  $("moderar-erro").hidden = true;
  $("moderar-escolha").hidden = false;
  if ($("moderar-vazio").hidden) $("moderar-quem").focus();
  else $("moderar-fechar").focus();
}

// ---------------------------------------------------------------- navegação

/**
 * Abre a caixa. `alvo` é o id de quem a porta nomeou, ou `null`.
 *
 * O foco entra e o anúncio sai, como em toda troca de tela desta janela — uma
 * caixa modal que aparece sem levar o teclado junto deixa quem navega por
 * teclado no elemento de trás, apertando coisas que não estão mais na frente.
 */
async function abrirModeracao(alvo) {
  let snapshot = null;
  try {
    snapshot = await invoke("snapshot");
  } catch (falha) {
    if (falha !== "NotConnected") console.warn("moderar:", falha);
  }
  if (!snapshot) return;

  focoAntesDeModerar = document.activeElement;
  alvoPreferido = alvo ?? null;
  caixaComEscolha = true;
  atoArmado = null;
  $("moderar-titulo").textContent = "MODERAÇÃO";
  $("moderar-confirmacao").hidden = true;
  $("moderar-escolha").hidden = false;
  $("moderar-erro").hidden = true;
  $("moderar-motivo").value = "";
  desenharModeracao(snapshot);

  $("moderar").hidden = false;
  if ($("moderar-vazio").hidden) $("moderar-quem").focus();
  else $("moderar-fechar").focus();
  anunciar("Moderação. Escolha a pessoa e o que fazer com ela.");
}

/**
 * A caixa aberta já confirmando, para um ato que não precisa escolher ninguém.
 *
 * É o caminho da mensagem: quem foi clicado já é o objeto, e uma lista de
 * pilotos no meio do caminho seria uma pergunta já respondida.
 */
function abrirConfirmacao(titulo, consequencia, rotulo, executar) {
  focoAntesDeModerar = document.activeElement;
  alvoPreferido = null;
  caixaComEscolha = false;
  $("moderar-titulo").textContent = titulo;
  $("moderar-erro").hidden = true;
  $("moderar").hidden = false;
  armarAto(rotulo, consequencia, executar);
  anunciar(consequencia);
}

/** Fecha e devolve o teclado a quem abriu. */
function fecharModeracao() {
  $("moderar").hidden = true;
  atoArmado = null;
  alvoPreferido = null;
  caixaComEscolha = false;
  $("moderar-confirmacao").hidden = true;
  $("moderar-escolha").hidden = false;
  $("moderar-erro").hidden = true;
  // A mesma conferência de `voltarParaTela`: o botão que abriu esta caixa pode
  // ter sido arrancado pelo redesenho da lista enquanto ela estava aberta, e
  // `focus()` num nó fora da árvore não faz nada e não avisa.
  if (focavel(focoAntesDeModerar)) focoAntesDeModerar.focus();
  else $("tela-sessao").focus();
  focoAntesDeModerar = null;
}

/**
 * Some sem devolver ninguém, e sem tocar no foco.
 *
 * Para quem já está trocando de tela por cima: a sessão pode acabar com a caixa
 * aberta — é justamente quem está moderando que um operador de cima derruba — e
 * `mostrarFim` escolhe a tela seguinte por conta própria. Devolver o foco ao
 * botão de trás ali seria pô-lo numa tela que acabou de ser escondida, e deixar
 * a caixa aberta a faria reaparecer sobre a **próxima** sessão, armada com um
 * ato sobre alguém de um Dogma que já ficou para trás.
 *
 * Mesma razão e mesma forma que `abandonarDogma` e `abandonarChamada`.
 */
function abandonarModeracao() {
  $("moderar").hidden = true;
  atoArmado = null;
  alvoPreferido = null;
  caixaComEscolha = false;
  focoAntesDeModerar = null;
  $("moderar-confirmacao").hidden = true;
  $("moderar-escolha").hidden = false;
  $("moderar-erro").hidden = true;
}

// ------------------------------------------------------------------ os atos

/**
 * A frase de consequência de expulsar.
 *
 * Diz o que acontece, quem vê, e — a metade que as pessoas erram — o que
 * **não** acontece: expulsar não é banir, e quem foi expulso pode entrar de
 * novo em seguida. Sem essa linha, quem quer barrar alguém expulsa três vezes e
 * conclui que o produto está quebrado.
 */
function consequenciaDeExpulsar(quem) {
  return (
    `${quem.nome} sai desta sessão agora, no meio do que estiver fazendo, e ` +
    `quem estiver em ${quem.cage} vê a saída.\n` +
    `Não é banimento: ${quem.nome} pode entrar de novo em seguida.`
  );
}

/**
 * A frase de consequência de banir, que muda com o prazo — e é por isso que o
 * prazo existe nesta tela.
 *
 * Banir para sempre é o único ato deste produto que **nenhuma tela dele
 * desfaz**: não há verbo de `unban` no protocolo, então o que foi barrado só
 * volta pelas mãos de quem tem acesso ao arquivo do Dogma no computador que o
 * hospeda. Um prazo é o único banimento que se desfaz sozinho, e a frase diz
 * qual dos dois está prestes a sair.
 */
function consequenciaDeBanir(quem, ate) {
  const comum =
    `${quem.nome} sai desta sessão agora e não consegue entrar de novo neste Dogma.\n`;
  if (ate === null) {
    return (
      comum +
      "É para sempre. Nenhuma tela deste produto desfaz um banimento: só quem " +
      "tiver acesso ao arquivo do Dogma, à mão, no computador que o hospeda."
    );
  }
  return (
    comum +
    `A barreira cai sozinha em ${new Date(ate * 1000).toLocaleString()}, pelo relógio ` +
    "deste computador — depois disso a pessoa entra de novo sem que ninguém faça nada."
  );
}

/**
 * A frase de consequência de mover.
 *
 * O ato mais fácil de subestimar dos quatro: ninguém é desconectado, e por isso
 * parece pequeno. Ele tira uma pessoa de onde ela estava sem que ela tenha
 * pedido, e é preciso dizer as três coisas que acontecem — ela recebe um aviso,
 * a sala de onde ela saiu a vê sair, e a sala para onde foi a vê entrar.
 */
function consequenciaDeMover(quem, destino) {
  return (
    `${quem.nome} é tirada de ${quem.cage} e posta em ${destino} sem ter pedido, ` +
    `e continua na sessão.\n` +
    `Ela recebe um aviso de que foi movida; quem está em ${quem.cage} a vê sair, ` +
    `e quem está em ${destino} a vê entrar.`
  );
}

// ------------------------------------------------------------------- ligação

// Delegação, e não um ouvinte por botão: as linhas dos Cages são refeitas a
// cada snapshot, duas vezes por segundo, e um ouvinte por botão seria
// registrado de novo a cada quadro. A mesma técnica de `alternarCanal`.
$("lista-cages").addEventListener("click", (evento) => {
  const alvo = evento.target.closest("button[data-moderar-piloto]");
  if (!alvo) return;
  abrirModeracao(alvo.dataset.moderarPiloto);
});

$("lista-mensagens").addEventListener("click", (evento) => {
  const alvo = evento.target.closest("button[data-moderar-mensagem]");
  if (!alvo) return;
  const mensagem = mensagens.find((m) => String(m.id) === alvo.dataset.moderarMensagem);
  if (!mensagem) return;
  abrirConfirmacao(
    "REMOVER MENSAGEM",
    `A mensagem de ${mensagem.author_nickname} das ${relogio(mensagem.at_seconds)} some ` +
      `desta Linha para todo mundo, e não há como trazê-la de volta.\n` +
      `«${mensagem.body}»`,
    "REMOVER",
    () => invoke("remover_mensagem", { message: mensagem.id }),
  );
});

$("alerta-ejetar").addEventListener("click", () => {
  // O alerta some junto: ele não é sobre a pessoa que está prestes a ser
  // escolhida — o `Notice` não carrega sujeito nenhum — e deixá-lo aceso atrás
  // da caixa sugeriria que é.
  $("banner").hidden = true;
  abrirModeracao(null);
});

$("moderar-expulsar").addEventListener("click", () => {
  const quem = quemEstaEscolhido();
  if (!quem) return;
  armarAto(`EXPULSAR ${quem.nome}`, consequenciaDeExpulsar(quem), () =>
    invoke("expulsar_piloto", { pilot: quem.id }),
  );
});

$("moderar-banir").addEventListener("click", () => {
  const quem = quemEstaEscolhido();
  if (!quem) return;
  const dias = Number($("moderar-ate").value);
  // O comando quer um instante absoluto em segundos, e o único relógio que esta
  // janela tem é o desta máquina — a frase de consequência diz isso, porque um
  // computador adiantado barra por mais tempo do que quem apertou escolheu.
  const ate = dias > 0 ? Math.floor(Date.now() / 1000) + dias * SEGUNDOS_POR_DIA : null;
  const motivo = $("moderar-motivo").value.trim();
  armarAto(`BANIR ${quem.nome}`, consequenciaDeBanir(quem, ate), () =>
    invoke("banir_piloto", {
      pilot: quem.id,
      // O motivo é para o registro de quem hospeda e nunca chega a quem foi
      // banido. Vazio vira `null`, que é como o Rust escreve «não houve».
      reason: motivo === "" ? null : motivo,
      expiresAt: ate,
    }),
  );
});

$("moderar-mover").addEventListener("click", () => {
  const quem = quemEstaEscolhido();
  if (!quem) return;
  const para = $("moderar-para");
  const destino = para.options[para.selectedIndex]?.textContent ?? "";
  armarAto(`MOVER ${quem.nome}`, consequenciaDeMover(quem, destino), () =>
    invoke("mover_piloto", { pilot: quem.id, cage: Number(para.value) }),
  );
});

// Trocar de pessoa refaz os destinos: a jaula em que ela está não é destino, e
// quem está escolhido muda sem a caixa fechar.
$("moderar-quem").addEventListener("change", desenharDestinos);

$("moderar-cancelar").addEventListener("click", desarmarAto);
$("moderar-fechar").addEventListener("click", fecharModeracao);

$("moderar-confirmar").addEventListener("click", async () => {
  const ato = atoArmado;
  if (!ato) return;
  try {
    await ato.executar();
  } catch (falha) {
    // A caixa fica aberta com o erro dentro. Fechar levaria a frase embora
    // junto, e quem apertou ficaria sem saber se o ato saiu ou não.
    const erro = $("moderar-erro");
    erro.hidden = false;
    erro.textContent = fraseDeErro(falha);
    return;
  }
  fecharModeracao();
  // O pedido entrou na fila; quem responde é o Dogma, pelo mesmo evento de
  // sempre. Redesenhar aqui é o que tira a pessoa da lista no quadro seguinte
  // quando ele responde depressa.
  await atualizar();
});

// Escape fecha, como em toda coisa que se põe por cima nesta janela. Só com a
// caixa na frente, ou engoliria a tecla de quem está fechando uma busca.
window.addEventListener("keydown", (evento) => {
  if (evento.key === "Escape" && !$("moderar").hidden) {
    evento.preventDefault();
    fecharModeracao();
  }
});
