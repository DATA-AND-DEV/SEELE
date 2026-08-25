// SEELE — a moderação (`#moderar`), camada sobre a operação.
//
// Os quatro verbos que a `specs/04-servidor-seele.md` dá ao Comandante e ao
// Operador: expulsar, banir, remover mensagem e mover pessoa. O Rust os tinha
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
// um banimento hoje só se desfaz por quem tem acesso ao arquivo do servidor, à
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
// `may_remove_message`, `may_move_person`) dizem o que **oferecer**, e são
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
 * JavaScript e não tem `id` nenhum — o botão de moderar de um pessoa é
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
 * — ali não há nada atrás, e voltar para um formulário de pessoas que ninguém
 * pediu seria a caixa inventando um passo.
 */
let caixaComEscolha = false;

/** Onde cada pessoa está, por id, para as frases de consequência. */
const ondeEstaOPersono = new Map();

/**
 * As salas do servidor, como estavam quando a caixa abriu.
 *
 * Guardadas porque o destino de uma mudança depende de **quem** está escolhido:
 * a jaula em que a pessoa já está não é destino nenhum, e quem está escolhido
 * muda sem a caixa fechar. Uma cópia do instante da abertura e não uma leitura
 * a cada quadro — a lista por baixo é redesenhada duas vezes por segundo, e
 * refazer as opções nesse ritmo apagaria a escolha de quem está escolhendo.
 */
let salasDoServer = [];

/** Quantos dias vale cada opção de banimento. Vazio é para sempre. */
const SEGUNDOS_POR_DIA = 86400;

/**
 * Há algum verbo de moderação sobre gente nesta sessão?
 *
 * Os três que agem sobre uma pessoa. `may_remove_message` fica de fora de
 * propósito: quem só pode apagar mensagem não tem o que fazer nesta caixa, e
 * abri-la seria oferecer três controles apagados.
 */
function podeModerarPersonos(snapshot) {
  return Boolean(
    snapshot && (snapshot.may_kick || snapshot.may_ban || snapshot.may_move_person),
  );
}

/**
 * O botão que abre a moderação de uma pessoa, ou `null` quando não há o que
 * oferecer.
 *
 * Mora na linha de quem está dentro de uma sala de voz, que é o único lugar desta
 * janela que lista **todo mundo em toda sala**: o roster e a grade da chamada
 * mostram só a sala de voz ocupado, e moderar é justamente o que se faz com quem está
 * noutra sala.
 *
 * Nunca sobre si mesmo: expulsar-se é o `DESCONECTAR` do cabeçalho, banir-se
 * não é coisa que exista, e mover-se é o `ENTRAR NA SALA` da própria lista.
 */
function botaoDeModerar(pessoa, snapshot) {
  if (pessoa.is_self || !podeModerarPersonos(snapshot)) return null;
  const botao = elemento("button", "moderar-porta", "MODERAR");
  botao.type = "button";
  botao.dataset.moderarPersono = String(pessoa.id);
  // O nome no rótulo acessível porque `MODERAR` sozinho se repete uma vez por
  // pessoa, e uma lista de botões idênticos é uma lista que não se navega.
  botao.setAttribute("aria-label", `moderar ${pessoa.nickname}`);
  botao.title = `expulsar, banir ou mover ${pessoa.nickname}`;
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
 * O botão que destrói uma sala de voz, ou `null` quando não há o que oferecer.
 *
 * `may_delete_rooms` e não `may_manage_voice_rooms`, e a diferença é a decisão inteira:
 * fazer uma sala é um erro que o servidor sobrevive, destruir uma acaba com o que
 * outra pessoa escreveu. A `specs/04-servidor-seele.md` enumera
 * `gerenciar_voice_rooms` e `administrar_server` separados justamente para que exista
 * um papel que ergue salas sem poder derrubá-las.
 *
 * ---- o último VoiceRoom vem desabilitado, e não escondido ----
 *
 * Um servidor sem VoiceRoom nenhum não tem onde falar, e o servidor recusa o pedido. Some
 * quem tem o objeto ausente — mover sem destino, moderar sem ninguém —, e aqui o
 * objeto existe: a sala está ali, e a razão de ela não poder ir embora é uma
 * coisa que se aprende lendo, não notando uma ausência.
 */
function botaoDeApagarVoiceRoom(voice_room, snapshot, ultimo) {
  if (snapshot.may_delete_rooms !== true) return null;
  const botao = elemento("button", "voice_room-apagar", "APAGAR");
  botao.type = "button";
  botao.dataset.apagarVoiceRoom = String(voice_room.id);
  botao.setAttribute("aria-label", `apagar a sala de voz ${voice_room.name}`);
  botao.disabled = ultimo;
  botao.title = ultimo
    ? `${voice_room.name} é a única sala de voz deste servidor, e um servidor sem sala de voz não tem onde falar. Faça outra antes.`
    : `destruir ${voice_room.name} e pôr para fora quem estiver dentro`;
  return botao;
}

/**
 * O botão que destrói uma Linha, ou `null` quando não há o que oferecer.
 *
 * Sem o par desabilitado da sala de voz: a última Linha pode ir. Um servidor sem Linha
 * nenhuma continua sendo o que este produto é — a `specs/04-servidor-seele.md`
 * faz a Linha presa a uma sala de voz **opcional**, então uma sala de voz sem texto é
 * uma configuração que o produto já prevê. Sem VoiceRoom não há onde falar; sem
 * Linha há.
 */
function botaoDeApagarLinha(linha, snapshot) {
  if (snapshot.may_delete_rooms !== true) return null;
  const botao = elemento("button", "linha-apagar", "APAGAR");
  botao.type = "button";
  botao.dataset.apagarLinha = String(linha.id);
  botao.setAttribute("aria-label", `apagar o canal ${linha.name}`);
  botao.title = `destruir #${linha.name} e tudo que foi escrito nela`;
  return botao;
}

/**
 * O segundo botão da caixa de alerta, que o comp desenha e o v2 deixou morto.
 *
 * A ambiguidade do rótulo do comp (Q6 do inventário — «ejetar o connection **do**
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
  const pode = podeModerarPersonos(snapshot);
  porta.disabled = !pode;
  porta.title = pode
    ? "escolher quem, e o que fazer: expulsar, banir ou mover"
    : "este servidor não deu a você nenhuma permissão de moderação";
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
  ondeEstaOPersono.clear();
  salasDoServer = snapshot.voice_rooms;

  const quem = $("moderar-quem");
  const opcoes = [];
  for (const voice_room of snapshot.voice_rooms) {
    for (const pessoa of voice_room.people) {
      if (pessoa.is_self) continue;
      ondeEstaOPersono.set(String(pessoa.id), {
        nome: pessoa.nickname,
        voice_room: voice_room.name,
        voice_roomId: voice_room.id,
      });
      const opcao = elemento("option", null, `${pessoa.nickname} — ${voice_room.name}`);
      opcao.value = String(pessoa.id);
      opcoes.push(opcao);
    }
  }
  repovoar(quem, opcoes);

  // Ninguém a moderar não é falha nem permissão que falta: é um servidor em que
  // todas as jaulas estão vazias, ou em que só você está dentro de uma. A frase
  // diz isso, e os três verbos somem — não ficam apagados, porque não há o que
  // explicar sobre um controle cujo objeto não existe.
  const ha = opcoes.length > 0;
  $("moderar-vazio").hidden = ha;
  $("moderar-sobre").hidden = !ha;
  if (ha) {
    // A porta que nomeou alguém pré-escolhe; a porta do alerta não nomeia
    // ninguém e cai na primeira pessoa da lista, que é o que um `<select>` faz.
    if (alvoPreferido && ondeEstaOPersono.has(alvoPreferido)) quem.value = alvoPreferido;
  }

  // Um verbo por permissão, e não os três por uma. `specs/04` enumera quatro
  // permissões e um papel pode ter qualquer subconjunto: quem só expulsa não vê
  // banir, e quem só move não vê nenhum dos dois.
  $("moderar-acao-expulsar").hidden = !snapshot.may_kick;
  $("moderar-acao-banir").hidden = !snapshot.may_ban;
  // Mover some também quando não há para onde: um servidor de uma jaula só não
  // tem destino, e um controle cujo objeto não existe é ruído, não lacuna.
  $("moderar-acao-mover").hidden = !snapshot.may_move_person || desenharDestinos() === 0;
}

/**
 * As jaulas para onde dá para mover quem está escolhido, e quantas são.
 *
 * A jaula em que a pessoa já está fica de fora: mover alguém para onde ela já
 * está não faz nada, e uma opção que não faz nada é uma opção que alguém vai
 * escolher — e depois vai perguntar por que não aconteceu nada.
 */
function desenharDestinos() {
  const escolhido = ondeEstaOPersono.get($("moderar-quem").value);
  const destinos = salasDoServer.filter((voice_room) => voice_room.id !== escolhido?.voice_roomId);
  repovoar(
    $("moderar-para"),
    destinos.map((voice_room) => {
      const opcao = elemento("option", null, `${voice_room.name} — ${voice_room.people.length}/${voice_room.limit}`);
      opcao.value = String(voice_room.id);
      return opcao;
    }),
  );
  return destinos.length;
}

/** Quem está escolhido agora, com o nome e a sala em que está. */
function quemEstaEscolhido() {
  const id = $("moderar-quem").value;
  const onde = ondeEstaOPersono.get(id);
  if (!onde) return null;
  return { id: Number(id), nome: onde.nome, voice_room: onde.voice_room };
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
  // Revelado aqui, e não só escondido em `abrirRecusa`: aquela é a única caixa
  // deste arquivo sem ato, e o botão que ela esconde tem que voltar para a
  // próxima que tiver um. Aqui porque é por onde os atos passam — todos.
  $("moderar-confirmar").hidden = false;
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
 * O estado do servidor agora, ou `null` quando não há sessão.
 *
 * Toda porta desta camada começa por aqui, e nenhuma delas desenha a partir do
 * que a lista de trás mostrava: a lista é redesenhada duas vezes por segundo, e
 * a sala de voz que estava na linha clicada pode ter deixado de existir entre o clique
 * e este quadro. Uma caixa escrita a partir da tela seria uma caixa sobre uma
 * sala que já não está lá.
 *
 * `NotConnected` sai calado. A sessão acabar enquanto alguém escolhia o que
 * fazer não é falha desta camada, e a tela de fim já está subindo por cima.
 */
async function lerSnapshot() {
  try {
    return await invoke("snapshot");
  } catch (falha) {
    if (falha !== "NotConnected") console.warn("moderar:", falha);
    return null;
  }
}

/**
 * Abre a caixa. `alvo` é o id de quem a porta nomeou, ou `null`.
 *
 * O foco entra e o anúncio sai, como em toda troca de tela desta janela — uma
 * caixa modal que aparece sem levar o teclado junto deixa quem navega por
 * teclado no elemento de trás, apertando coisas que não estão mais na frente.
 */
async function abrirModeracao(alvo) {
  const snapshot = await lerSnapshot();
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
 * pessoas no meio do caminho seria uma pergunta já respondida.
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

/**
 * A caixa aberta dizendo por que **não** vai perguntar nada.
 *
 * Existe por causa de um caso só, e é o caso que decide a honestidade desta
 * tela: a contagem de uma Linha não chegou. Sem os três números não há versão
 * honesta da confirmação — o que sobraria é «apagar a Linha?», que é a pergunta
 * que não informa nada e treina a apertar duas vezes.
 *
 * Então nada é armado. A caixa aparece com a frase e um botão de fechar, e o
 * ato não existe: não há `atoArmado`, logo o CONFIRMAR não tem o que rodar, e
 * ele sai da frente para não haver o que apertar por engano.
 */
function abrirRecusa(titulo, frase) {
  focoAntesDeModerar = document.activeElement;
  alvoPreferido = null;
  caixaComEscolha = false;
  atoArmado = null;
  $("moderar-titulo").textContent = titulo;
  $("moderar-escolha").hidden = true;
  $("moderar-confirmacao").hidden = false;
  $("moderar-consequencia").textContent = frase;
  $("moderar-confirmar").hidden = true;
  $("moderar-erro").hidden = true;
  $("moderar").hidden = false;
  $("moderar-cancelar").focus();
  anunciar(frase);
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
 * ato sobre alguém de um servidor que já ficou para trás.
 *
 * Mesma razão e mesma forma que `abandonarServer` e `abandonarChamada`.
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
    `quem estiver em ${quem.voice_room} vê a saída.\n` +
    `Não é banimento: ${quem.nome} pode entrar de novo em seguida.`
  );
}

/**
 * A frase de consequência de banir, que muda com o prazo — e é por isso que o
 * prazo existe nesta tela.
 *
 * Banir para sempre é o único ato deste produto que **nenhuma tela dele
 * desfaz**: não há verbo de `unban` no protocolo, então o que foi barrado só
 * volta pelas mãos de quem tem acesso ao arquivo do servidor no computador que o
 * hospeda. Um prazo é o único banimento que se desfaz sozinho, e a frase diz
 * qual dos dois está prestes a sair.
 */
function consequenciaDeBanir(quem, ate) {
  const comum =
    `${quem.nome} sai desta sessão agora e não consegue entrar de novo neste servidor.\n`;
  if (ate === null) {
    return (
      comum +
      "É para sempre. Nenhuma tela deste produto desfaz um banimento: só quem " +
      "tiver acesso ao arquivo do servidor, à mão, no computador que o hospeda."
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
    `${quem.nome} é tirada de ${quem.voice_room} e posta em ${destino} sem ter pedido, ` +
    `e continua na sessão.\n` +
    `Ela recebe um aviso de que foi movida; quem está em ${quem.voice_room} a vê sair, ` +
    `e quem está em ${destino} a vê entrar.`
  );
}

/**
 * A frase de consequência de apagar uma sala de voz.
 *
 * Diz as três coisas que quem aperta não vive: quanta gente é posta para fora,
 * que isso acontece no meio do que estiverem falando, e que cada uma é avisada.
 * E a que mais engana: a Linha presa à sala de voz **não** vai junto. Sem essa linha,
 * quem quer acabar com uma conversa apaga a sala de voz, olha a Linha ainda ali, e
 * conclui que o produto não fez o que disse.
 */
function consequenciaDeApagarVoiceRoom(voice_room, linhaPresa) {
  const dentro = voice_room.people.length;
  const gente =
    dentro === 0
      ? `Não há ninguém dentro de ${voice_room.name} agora.`
      : `Isto põe ${dentro} ${dentro === 1 ? "pessoa" : "pessoas"} para fora de ` +
        `${voice_room.name} agora, no meio do que estiverem falando, e cada uma recebe ` +
        `um aviso dizendo o que aconteceu.`;
  const texto = linhaPresa
    ? `\nO canal #${linhaPresa.name} continua, com tudo que foi escrito nele: ele não é apagado junto.`
    : "";
  return `${gente}${texto}\nNenhuma tela deste produto traz esta sala de voz de volta.`;
}

/**
 * A frase de consequência de apagar uma Linha.
 *
 * Os três números são requisito e não enfeite: quantas mensagens, de quanta
 * gente, desde quando. Eles vêm de `peso_da_linha`, contados no banco do servidor
 * no instante de perguntar — esta janela segura uma página de histórico e
 * chutaria para baixo por todo o passado da Linha, e um número quase certo numa
 * caixa que promete destruir mil e oitocentas mensagens é pior que nenhum.
 *
 * A Linha vazia tem ramo próprio porque não tem data para dar: «escritas desde»
 * não existe quando ninguém escreveu.
 */
function consequenciaDeApagarLinha(linha, peso, voice_roomsPresos) {
  const escrito =
    peso.messages === 0
      ? `Ninguém escreveu nada em #${linha.name} ainda.`
      : `Isto destrói ${peso.messages.toLocaleString()} ` +
        `${peso.messages === 1 ? "mensagem" : "mensagens"} de ${peso.authors} ` +
        `${peso.authors === 1 ? "pessoa" : "pessoas"}, ` +
        `${peso.messages === 1 ? "escrita" : "escritas"} desde ` +
        `${dataDeInicio(peso.oldest_at_seconds)}. Nenhuma tela deste produto ` +
        `as traz de volta.`;
  // Uma sala de voz que apontava para esta Linha sobrevive e sai sem Linha nenhuma. É
  // uma mudança que ninguém pediu, e as caixas deste produto nomeiam o que
  // fazem.
  const soltos =
    voice_roomsPresos.length === 0
      ? ""
      : `\n${voice_roomsPresos.map((voice_room) => voice_room.name).join(", ")} ` +
        `${voice_roomsPresos.length === 1 ? "fica" : "ficam"} sem canal, e ` +
        `${voice_roomsPresos.length === 1 ? "continua" : "continuam"} de pé.`;
  return (
    `${escrito}${soltos}\n` +
    `Quem estiver lendo #${linha.name} agora a perde da tela no mesmo instante.`
  );
}

/**
 * A data a partir da qual há coisa escrita, como uma pessoa a lê.
 *
 * O ano entra quando não é este. `12/03` é o que se quer ler quando foi este
 * ano e é uma mentira por omissão quando foi há três — e o número que a caixa
 * promete destruir merece uma data que não precise de contexto.
 */
function dataDeInicio(segundos) {
  if (!segundos) return "sempre";
  const quandoFoi = new Date(segundos * 1000);
  const mesmoAno = quandoFoi.getFullYear() === new Date().getFullYear();
  return quandoFoi.toLocaleDateString(
    [],
    mesmoAno
      ? { day: "2-digit", month: "2-digit" }
      : { day: "2-digit", month: "2-digit", year: "numeric" },
  );
}

// ------------------------------------------------------------------- ligação

// Delegação, e não um ouvinte por botão: as linhas das salas de voz são refeitas a
// cada snapshot, duas vezes por segundo, e um ouvinte por botão seria
// registrado de novo a cada quadro. A mesma técnica de `alternarCanal`.
$("lista-voice_rooms").addEventListener("click", (evento) => {
  const alvo = evento.target.closest("button[data-moderar-pessoa]");
  if (!alvo) return;
  abrirModeracao(alvo.dataset.moderarPersono);
});

$("lista-mensagens").addEventListener("click", (evento) => {
  const alvo = evento.target.closest("button[data-moderar-mensagem]");
  if (!alvo) return;
  const mensagem = mensagens.find((m) => String(m.id) === alvo.dataset.moderarMensagem);
  if (!mensagem) return;
  abrirConfirmacao(
    "REMOVER MENSAGEM",
    `A mensagem de ${mensagem.author_nickname} das ${relogio(mensagem.at_seconds)} some ` +
      `deste canal para todo mundo, e não há como trazê-la de volta.\n` +
      `«${mensagem.body}»`,
    "REMOVER",
    () => invoke("remover_mensagem", { message: mensagem.id }),
  );
});

// Apagar uma sala passa por esta mesma máquina, e não por uma caixa própria: a
// regra é a mesma — dizer a consequência **é** a confirmação — e uma segunda
// forma de confirmar seria uma segunda forma de esquecer de escrever a frase.
//
// Delegação como as duas de cima, e pelo mesmo motivo: as duas listas são
// refeitas a cada snapshot, duas vezes por segundo.
$("lista-voice_rooms").addEventListener("click", async (evento) => {
  const alvo = evento.target.closest("button[data-apagar-voice_room]");
  if (!alvo) return;
  const id = Number(alvo.dataset.apagarVoiceRoom);
  const snapshot = await lerSnapshot();
  if (!snapshot) return;
  const voice_room = snapshot.voice_rooms.find((c) => c.id === id);
  if (!voice_room) return;
  // A Linha presa é lida da sala de voz, e não adivinhada pelo nome: um servidor pode ter
  // uma Linha chamada como a sala e nenhuma ligação entre as duas.
  const presa = snapshot.channels.find((linha) => linha.id === voice_room.channel) ?? null;
  abrirConfirmacao(
    `APAGAR A SALA DE VOZ ${voice_room.name} ?`,
    consequenciaDeApagarVoiceRoom(voice_room, presa),
    `APAGAR ${voice_room.name}`,
    // camelCase: ver a nota em `tela-sessao.js`, no `insert_plug`.
    () => invoke("apagar_voice_room", { voiceRoom: id }),
  );
});

$("lista-linhas").addEventListener("click", async (evento) => {
  const alvo = evento.target.closest("button[data-apagar-linha]");
  if (!alvo) return;
  const id = Number(alvo.dataset.apagarLinha);
  const snapshot = await lerSnapshot();
  if (!snapshot) return;
  const linha = snapshot.channels.find((l) => l.id === id);
  if (!linha) return;

  // O peso vem antes da caixa, e a caixa não abre sem ele. Os três números são
  // a confirmação inteira; sem eles o que sobraria é «apagar a Linha?», que é a
  // pergunta que não informa nada.
  let peso = null;
  try {
    peso = await invoke("peso_da_linha", { channel: id });
  } catch (falha) {
    abrirRecusa(
      `APAGAR O CANAL #${linha.name} ?`,
      `Não consegui contar o que há dentro de #${linha.name}, então não vou ` +
        `perguntar se você quer apagá-lo.\n` +
        `Esta caixa existe para dizer quantas mensagens, de quanta gente e ` +
        `desde quando — e um número inventado seria pior que nenhum.\n` +
        `${fraseDeErro(falha)}`,
    );
    return;
  }

  abrirConfirmacao(
    `APAGAR O CANAL #${linha.name} ?`,
    consequenciaDeApagarLinha(
      linha,
      peso,
      snapshot.voice_rooms.filter((voice_room) => voice_room.channel === id),
    ),
    `APAGAR #${linha.name}`,
    () => invoke("apagar_linha", { channel: id }),
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
    invoke("expulsar_pessoa", { person: quem.id }),
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
    invoke("banir_pessoa", {
      person: quem.id,
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
    invoke("mover_pessoa", { person: quem.id, voiceRoom: Number(para.value) }),
  );
});

// Trocar de pessoa refaz os destinos: a jaula em que ela está não é destino, e
// quem está escolhido muda sem a caixa fechar.
$("moderar-quem").addEventListener("change", desenharDestinos);

$("moderar-cancelar").addEventListener("click", desarmarAto);
$("moderar-fechar").addEventListener("click", fecharModeracao);
fecharAoClicarFora("moderar", fecharModeracao);

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
  // O pedido entrou na fila; quem responde é o servidor, pelo mesmo evento de
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
