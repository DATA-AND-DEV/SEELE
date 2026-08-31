// SEELE — compartilhar a tela (`#compartilhar`).
//
// A caixa que escolhe **o que** sai desta máquina e **até onde** ele sobe. Ela
// não desenha o que está saindo: isso é do palco, em `tela-chamada.js`, e a
// divisão é de propósito — o que está saindo tem de continuar legível com esta
// caixa fechada.
//
// ---- o que este arquivo não decide ----
//
// O teto. A decisão de 22/08 da spec de compartilhamento (§5.1) o escreve como
//
//     min(caminho de quem HOSPEDA × 60% ÷ N espectadores,
//         caminho de quem COMPARTILHA × 60%,
//         a escolha da pessoa)
//
// e as duas primeiras linhas são medidas que esta janela não tem. O que sai
// daqui é a terceira, e ela é **teto e nunca piso** (§5): o sistema continua
// livre para ficar abaixo, e o palco mostra o resultado ao lado do pedido.
//
// ---- por que a permissão é um comando à parte da lista ----
//
// `fontes_de_tela` devolve vazio quando o sistema recusou, e vazio também é a
// resposta de uma máquina que não tem o que oferecer. Sem `permissao_de_tela`
// esta caixa teria de adivinhar entre as duas, e uma lista vazia sem motivo é
// um beco.
//
// ---- o que vem de fora ----
//
// `anunciar`, `focavel`, `elemento`, `repovoar` e `invoke` são de `base.js`;
// `fraseDeErro` e `FRASES` são de `frases.js`. `nomeDaResolucao` é declarada
// **aqui** e lida por `tela-chamada.js`, do mesmo jeito que aquele arquivo já lê
// `medido` de `tela-sessao.js` — os scripts dividem um escopo só (ADR 0019).
//
// ---- o que esta caixa deixou de guardar ----
//
// Os limites que ela mandou. Eles moravam aqui, numa variável desta janela, e
// morriam com ela: uma recarga no meio de uma transmissão apagava metade da
// comparação que o §5 obriga — o que está saindo ao lado do que foi pedido — e
// o palco escrevia travessão sobre uma tela que continuava saindo. Agora o
// pedido volta em `Snapshot::tela.pedido`, guardado do lado que sobrevive à
// janela, que é o mesmo lado que o manda ao codificador.

"use strict";

/** Onde o teclado estava quando esta caixa abriu. */
let focoAntesDeCompartilhar = null;

/**
 * A fonte que a pessoa escolheu na lista, ou `null`.
 *
 * Número e não texto porque é isso que `compartilhar_tela` recebe. O `id` de
 * uma fonte é um `u64` do lado de lá e um `double` deste, o que dá 2^53 de
 * folga — mais do que qualquer identificador de janela dos três sistemas usa, e
 * ainda assim uma folga e não uma garantia. Se um dia um sistema devolver um
 * identificador maior que isso, o sintoma é uma fonte que não abre e nada
 * dizendo por quê.
 */
let fonteArmada = null;

/**
 * A última resposta do sistema sobre gravar a tela, ou `null` antes da primeira.
 *
 * Guardada aqui e lida pela chamada porque é ela que decide se o botão
 * COMPARTILHAR A TELA existe: `NaoSeSabe` quer dizer que esta compilação não
 * tem como perguntar ao sistema, e uma casca que recebe isso **não desenha o
 * controle** — é a mesma resposta que `Snapshot::caminho` dá com `None`.
 *
 * Conferida ao abrir a chamada e depois de cada pedido, e não num laço: uma
 * permissão de sistema muda quando alguém vai aos ajustes, o que não acontece
 * duas vezes por segundo.
 */
let permissaoDeTela = null;

/**
 * A última falha que esta caixa tem para mostrar, ou `null`.
 *
 * Guardada em vez de escrita direto no nó porque a caixa se redesenha depois de
 * cada aperto: escrever no nó faria o próprio redesenho que segue a recusa
 * apagá-la, e quem apertou ficaria com um botão que não fez nada e nenhuma
 * frase dizendo por quê.
 */
let erroDeTela = null;

/** Pergunta ao sistema o que ele responde sobre gravar a tela, e guarda. */
async function conferirPermissaoDeTela() {
  try {
    permissaoDeTela = await invoke("permissao_de_tela");
  } catch (falha) {
    // Sem sessão não há a quem perguntar, e isso não é uma resposta sobre
    // permissão nenhuma: o controle fica de fora até haver.
    if (falha !== "NotConnected") console.warn("permissao_de_tela:", falha);
    permissaoDeTela = null;
  }
  return permissaoDeTela;
}

/**
 * Se esta janela deve oferecer o controle de compartilhar.
 *
 * `null` — ainda não perguntado — conta como não: um botão que aparece meio
 * segundo depois da tela é um botão que ninguém viu chegar, e a chamada
 * pergunta antes de desenhar pela primeira vez.
 */
function temControleDeTela() {
  return permissaoDeTela !== null && permissaoDeTela !== "NaoSeSabe";
}

/**
 * O que uma altura de imagem se chama para quem lê.
 *
 * Declarada aqui e usada também pelo palco. A lista fechada do §5 é 1080p,
 * 720p e 540p; o `${altura}p` cobre qualquer outra que o codificador escolha
 * por baixo, porque o teto é da pessoa e o que sai é do sistema.
 */
function nomeDaResolucao(altura) {
  return `${altura}p`;
}

/**
 * O que dizer sobre a permissão do sistema, por resposta.
 *
 * `Concedida` não tem entrada e não é esquecimento: repetir «está tudo certo» a
 * cada abertura é ensinar a não ler a linha no dia em que ela não estiver.
 *
 * A frase de `Negada` nomeia dois sistemas porque eles pedem coisas diferentes
 * de quem lê. No macOS o TCC não pergunta duas vezes e o conserto é fora deste
 * app; no Linux quem decide é o compositor, e ele volta a perguntar sozinho.
 * Uma frase só mandaria metade das pessoas ao lugar errado.
 */
// A resposta do macOS é **guardada pelo processo**, e isso decide o que estas
// frases precisam dizer.
//
// `CGPreflightScreenCaptureAccess` responde uma vez e repete a mesma resposta
// até o app reabrir. Quem concede nos Ajustes com o SEELE aberto vê a tela
// continuar dizendo que não tem permissão, e lê isso como app quebrado — foi o
// que aconteceu no primeiro teste de campo da 0.7.10, depois de a chave do
// `Info.plist` já estar no lugar.
//
// Por isso as duas frases que precedem uma concessão mandam reabrir, e não só a
// de recusa: quem chega pelo caminho «ainda não pediu» e resolve pelos ajustes
// cai no mesmo cache.
const PERMISSAO_DE_TELA = {
  NaoPerguntada: {
    diz: "ESTE APP AINDA NÃO PEDIU PARA GRAVAR A TELA",
    // Sem `**` nem nenhuma outra marcação: isto é escrito com `textContent`
    // (ver `mostrarPermissao`), então marcação aparece literal na tela. Foi o
    // que aconteceu aqui.
    nota:
      "O sistema pergunta uma vez. Se você recusar, a volta é pelos ajustes " +
      "dele — e depois de marcar lá, feche e abra o SEELE, ou ele continua " +
      "achando que não tem permissão.",
    pedir: true,
  },
  // Esta compilação não tem como perguntar ao sistema. A chamada nem desenha o
  // botão que abre esta caixa nesse caso; a entrada existe para quando a
  // resposta mudar entre a conferência e a abertura, que é o único jeito de
  // alguém chegar aqui e não poder fazer nada.
  NaoSeSabe: {
    diz: "ESTA VERSÃO NÃO SABE COMPARTILHAR TELA",
    nota:
      "Não é a sua conexão nem permissão: a parte que captura a tela ainda não " +
      "está neste app.",
    pedir: false,
  },
  Negada: {
    diz: "O SISTEMA NEGOU A GRAVAÇÃO DE TELA",
    nota:
      "No macOS ele não pergunta de novo: abra Ajustes do Sistema · " +
      "Privacidade e Segurança · Gravação de Tela, marque o SEELE e reabra o " +
      "app. No Linux quem decide é o compositor, e ele pergunta de novo na " +
      "próxima tentativa.",
    pedir: false,
  },
};

// ------------------------------------------------------------------- desenho

/**
 * A caixa inteira, a partir do que a máquina e a sessão respondem agora.
 *
 * Três perguntas e nesta ordem: a permissão, porque ela explica uma lista
 * vazia; a lista; e o snapshot, porque é ele que diz se já há transmissão nesta
 * sala e de quem ela é.
 */
async function desenharCompartilhar() {
  const permissao = await conferirPermissaoDeTela();
  desenharPermissaoDeTela(permissao);
  await desenharModuloDeTela();

  let fontes = [];
  let listou = true;
  try {
    fontes = await invoke("fontes_de_tela");
  } catch (falha) {
    listou = false;
    mostrarErroDeTela(falha);
  }
  desenharFontesDeTela(fontes, permissao, listou);

  let snapshot = null;
  try {
    snapshot = await invoke("snapshot");
  } catch (falha) {
    if (falha !== "NotConnected") console.warn("snapshot:", falha);
  }
  desenharBotoesDeTela(snapshot);
}

/**
 * O bloco do módulo de vídeo, ou nada quando não há o que baixar.
 *
 * `null` cobre os dois casos em que não há oferta a fazer — já está instalado, e
 * este sistema não tem módulo publicado. A caixa some nos dois, e é o certo:
 * quem está no Linux não ganha nada vendo um botão que não tem o que buscar.
 */
async function desenharModuloDeTela() {
  const bloco = $("compartilhar-modulo");
  let oferta = null;
  try {
    oferta = await invoke("modulo_de_video_a_baixar");
  } catch (falha) {
    console.warn("modulo_de_video_a_baixar:", falha);
  }
  bloco.hidden = !oferta;
  if (!oferta) return;

  // O tamanho arredondado, e a origem inteira. Meio megabyte é uma decisão que
  // se toma sem pensar; «um componente» não é.
  const mb = (oferta.bytes / (1024 * 1024)).toFixed(1).replace(".", ",");
  $("compartilhar-modulo-onde").textContent = `${mb} MB — ${oferta.url}`;
}

/** O bloco da permissão, ou nada quando não há o que dizer sobre ela. */
function desenharPermissaoDeTela(permissao) {
  const bloco = $("compartilhar-permissao");
  const dito = PERMISSAO_DE_TELA[permissao];
  bloco.hidden = !dito;
  $("compartilhar-pedir").hidden = !dito || !dito.pedir;
  if (!dito) return;

  $("compartilhar-permissao-diz").textContent = dito.diz;
  $("compartilhar-permissao-nota").textContent = dito.nota;
}

/**
 * A lista de monitores e janelas, ou a frase do vazio.
 *
 * Vazio tem três causas e é por isso que esta função recebe a permissão e o
 * `listou` junto: «o sistema disse não» tem conserto nos ajustes, «esta máquina
 * não ofereceu nada» não tem, e «o pedido falhou» já está escrito na linha de
 * erro. As três embaixo de uma lista vazia são indistinguíveis.
 */
function desenharFontesDeTela(fontes, permissao, listou) {
  const lista = $("compartilhar-fontes");
  const vazio = $("compartilhar-sem-fonte");

  if (fontes.length === 0) {
    repovoar(lista, []);
    // Lista que nem chegou a ser pedida com sucesso não ganha frase daqui: a
    // falha já está escrita embaixo, e duas explicações para uma lista vazia é
    // uma delas estando errada.
    vazio.hidden = !listou;
    vazio.textContent =
      permissao === "Concedida"
        ? "Esta máquina não ofereceu nenhuma tela nem nenhuma janela para transmitir."
        : "A lista fica vazia enquanto o sistema não deixar gravar a tela.";
    return;
  }

  vazio.hidden = true;
  // A fonte armada pode ter sumido entre dois desenhos — uma janela fechada é
  // uma escolha que deixou de existir. Desarmar aqui é o que impede o botão de
  // mandar um identificador que ninguém mais reconhece.
  if (fonteArmada !== null && !fontes.some((fonte) => fonte.id === fonteArmada)) {
    fonteArmada = null;
  }

  repovoar(
    lista,
    fontes.map((fonte) => {
      const item = elemento("li");
      const botao = elemento("button", "compartilhar-fonte");
      botao.type = "button";
      botao.dataset.fonte = String(fonte.id);
      const armada = fonte.id === fonteArmada;
      // `aria-pressed` e a palavra `ESCOLHIDA` dizem a mesma coisa que a cor de
      // fundo diz. `specs/06-clientes-gui.md` proíbe informação transmitida só
      // por cor, e qual fonte está armada é informação.
      botao.setAttribute("aria-pressed", armada ? "true" : "false");
      botao.append(
        elemento("span", "compartilhar-fonte-nome", fonte.nome),
        elemento(
          "span",
          "compartilhar-fonte-diz",
          `${fonte.monitor ? "MONITOR" : "JANELA"} · ${fonte.largura}×${fonte.altura}` +
            (armada ? " · ESCOLHIDA" : ""),
        ),
      );
      item.append(botao);
      return item;
    }),
  );
}

/**
 * Os dois botões do rodapé, e a recusa dita **antes** do aperto.
 *
 * Cabe uma transmissão por sala de voz. Com outra pessoa transmitindo,
 * `compartilhar_tela` recusaria com `ScreenShareTaken` — e fazer alguém apertar
 * para descobrir isso é esconder do lado de fora o que já se sabe do lado de
 * dentro. O botão sai desabilitado com a frase escrita, que é a mesma frase da
 * recusa.
 */
function desenharBotoesDeTela(snapshot) {
  const tela = snapshot ? snapshot.tela : null;
  const minha = Boolean(tela && tela.e_minha);
  const deOutro = Boolean(tela) && !minha;

  const comecar = $("compartilhar-comecar");
  comecar.textContent = minha ? "APLICAR OS LIMITES" : "COMPARTILHAR";
  comecar.disabled = !snapshot || deOutro || (!minha && fonteArmada === null);
  comecar.title = deOutro
    ? FRASES.ScreenShareTaken
    : !minha && fonteArmada === null
      ? "escolha um monitor ou uma janela na lista acima"
      : "";

  $("compartilhar-parar").hidden = !minha;

  // A recusa de estar ocupada não espera aperto nenhum: ela já é verdade, e
  // uma falha guardada de um aperto anterior vem antes dela porque é a que
  // responde ao que a pessoa acabou de fazer.
  const dito = erroDeTela ?? (deOutro ? FRASES.ScreenShareTaken : null);
  const erro = $("compartilhar-erro");
  erro.hidden = dito === null;
  erro.textContent = dito ?? "";
}

/** Guarda uma falha para o próximo desenho escrever. */
function mostrarErroDeTela(falha) {
  erroDeTela = fraseDeErro(falha);
}

/**
 * Os três tetos, como o Rust os recebe.
 *
 * Nomes de campo em `snake_case` porque é assim que `LimitesDeTela` atravessa —
 * o mesmo acordo de `muted` e `sync_band` no snapshot. Banda vazia é `null`
 * e não zero: zero seria um teto de zero bit por segundo, que é o contrário de
 * «sem teto meu».
 */
function limitesEscolhidos() {
  const banda = $("compartilhar-banda").value;
  return {
    banda_bps: banda === "" ? null : Number(banda),
    altura_maxima: Number($("compartilhar-altura").value),
    quadros_maximos: Number($("compartilhar-quadros").value),
  };
}

// ---------------------------------------------------------------- navegação

/** Abre a caixa por cima da chamada. */
async function abrirCompartilhar() {
  focoAntesDeCompartilhar = document.activeElement;
  erroDeTela = null;
  await desenharCompartilhar();
  $("compartilhar").hidden = false;
  // O foco na caixa e não na lista: quem abriu ainda vai ler o que a permissão
  // diz, e um leitor de tela que começasse no meio da lista não anunciaria o
  // título que diz o que é isto.
  $("compartilhar").focus();
  anunciar(
    "Compartilhar a tela. Escolha um monitor ou uma janela, e os três tetos. Escape fecha.",
  );
}

/** Fecha, devolvendo o teclado a quem abriu. */
function fecharCompartilhar() {
  $("compartilhar").hidden = true;
  // A mesma conferência de `voltarParaTela`: a tela de baixo pode ter trocado
  // enquanto a caixa estava aberta, e `focus()` num nó fora da árvore não faz
  // nada e não reporta nada.
  if (focavel(focoAntesDeCompartilhar)) focoAntesDeCompartilhar.focus();
  focoAntesDeCompartilhar = null;
}

/**
 * Some sem devolver ninguém, e sem tocar no foco.
 *
 * A sessão pode acabar com a caixa aberta, e `mostrarFim` escolhe a tela
 * seguinte por conta própria. Mesma razão e mesma forma que `abandonarChamada`.
 */
function abandonarCompartilhar() {
  $("compartilhar").hidden = true;
  focoAntesDeCompartilhar = null;
  fonteArmada = null;
  erroDeTela = null;
}

// ------------------------------------------------------------------- ligação

$("compartilhar-fechar").addEventListener("click", fecharCompartilhar);
fecharAoClicarFora("compartilhar", fecharCompartilhar);

/**
 * A escolha da fonte, por delegação.
 *
 * Um ouvinte na lista e não um por botão: a lista é refeita a cada desenho, e
 * um ouvinte por botão teria que ser registrado de novo a cada volta.
 */
$("compartilhar-fontes").addEventListener("click", (evento) => {
  const alvo = evento.target.closest("[data-fonte]");
  if (!alvo) return;
  fonteArmada = Number(alvo.dataset.fonte);
  erroDeTela = null;
  desenharCompartilhar().catch((falha) => console.warn("fontes_de_tela:", falha));
});

/**
 * Pedir a permissão, e só por aperto.
 *
 * No macOS isto abre o alerta do TCC, que só aparece uma vez na vida do app: um
 * alerta de sistema que aparece sem ninguém ter pedido é o que ensina a recusar
 * por reflexo, e daquela recusa não se volta de dentro deste app.
 */
$("compartilhar-pedir").addEventListener("click", async () => {
  erroDeTela = null;
  try {
    permissaoDeTela = await invoke("pedir_permissao_de_tela");
  } catch (falha) {
    mostrarErroDeTela(falha);
  }
  await desenharCompartilhar();
  await atualizarChamada();
});

/**
 * Buscar o módulo, depois de a pessoa ter lido o tamanho e a origem e apertado.
 *
 * O botão vira texto durante a busca em vez de sumir: um botão que desaparece ao
 * ser apertado parece um clique que não pegou, e um megabyte numa conexão ruim
 * demora o suficiente para alguém apertar de novo.
 */
$("compartilhar-baixar").addEventListener("click", async () => {
  const botao = $("compartilhar-baixar");
  const antes = botao.textContent;
  botao.disabled = true;
  botao.textContent = "BAIXANDO…";
  erroDeTela = null;
  try {
    await invoke("baixar_modulo_de_video");
  } catch (falha) {
    mostrarErroDeTela(falha);
  }
  botao.disabled = false;
  botao.textContent = antes;
  await desenharCompartilhar();
});

$("compartilhar-comecar").addEventListener("click", async () => {
  const limites = limitesEscolhidos();
  const comecando = $("compartilhar-parar").hidden;
  erroDeTela = null;
  try {
    if (comecando) {
      await invoke("compartilhar_tela", { fonte: fonteArmada, limites });
    } else {
      // Já transmitindo: trocar o teto não recomeça a transmissão. Recomeçá-la
      // piscaria a imagem de todo mundo que está assistindo por causa de um
      // controle que uma pessoa mexeu.
      await invoke("ajustar_limites_da_tela", { limites });
    }
    if (comecando) {
      registrarEventoDaChamada("você começou a compartilhar a sua tela", "anotacao");
      // Começou: esta caixa some e a chamada entra.
      //
      // Quem acabou de escolher uma tela quer **ver** o que está saindo, e
      // ficar na caixa de escolha depois de escolher é a tela pedindo uma
      // decisão que já foi tomada. A chamada é onde a transmissão aparece —
      // a própria e a dos outros —, e é para lá que o gesto aponta.
      //
      // Só quando começa. Mexer no teto de uma transmissão em curso não é
      // motivo para trocar de tela: quem ajusta está olhando o controle, e
      // arrancá-lo do olho seria o oposto do que ele pediu.
      fecharCompartilhar();
      await abrirChamada();
    }
  } catch (falha) {
    mostrarErroDeTela(falha);
  }
  await desenharCompartilhar();
  await atualizarChamada();
});

$("compartilhar-parar").addEventListener("click", async () => {
  erroDeTela = null;
  try {
    await invoke("parar_de_compartilhar");
    registrarEventoDaChamada("você parou de compartilhar a tela", "anotacao");
  } catch (falha) {
    mostrarErroDeTela(falha);
  }
  await desenharCompartilhar();
  await atualizarChamada();
});

/**
 * A transmissão mudou de estado enquanto esta caixa está aberta.
 *
 * A chamada por baixo se redesenha sozinha duas vezes por segundo; esta caixa
 * não, e de propósito: ela é uma escolha e não uma medida que anda, e um laço
 * arrancaria a lista de fontes de baixo de quem está escolhendo uma. O evento é
 * o que a mantém honesta sem laço — alguém começou a compartilhar do outro
 * lado, e o botão daqui tem de dizer isso antes do aperto, e não depois.
 */
listen("seele://event", (evento) => {
  if (evento.payload !== "ScreenChanged" || $("compartilhar").hidden) return;
  desenharCompartilhar().catch((falha) => console.warn("ScreenChanged:", falha));
});

// Escape fecha, como em toda coisa que se põe por cima nesta janela. Só com a
// caixa na frente, ou engoliria a tecla de quem está fechando outra coisa.
window.addEventListener("keydown", (evento) => {
  if (evento.key === "Escape" && !$("compartilhar").hidden) {
    evento.preventDefault();
    evento.stopPropagation();
    fecharCompartilhar();
  }
});
