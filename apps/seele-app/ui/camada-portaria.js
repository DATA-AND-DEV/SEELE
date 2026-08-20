// SEELE · Entry Plug — a portaria: a porta do Dogma que esta janela hospeda.
//
// ADR 0030. O buraco que isto fecha, nas palavras do dono: «precisava ter uma
// tela de permissão para usuários não registrados nos dogmas, assim não entra
// qualquer um». O servidor sabia fechar a porta desde o ADR 0021 e o único
// caminho até lá era o terminal — que é exatamente o que o público do botão
// HOSPEDAR AQUI não abre.
//
// ---- as três camadas, e por que as três aparecem juntas ----
//
// Senha e convite decidem sobre um **segredo**; a portaria decide sobre
// **gente**. Elas são conjuntivas: passar por uma não dispensa a outra. Um
// painel que mostrasse uma de cada vez faria quem hospeda achar que fechou a
// porta ao pôr uma senha, quando o que ele fez foi fechar um dos três trincos.
// Por isso as três estão sempre na tela, sempre com o estado em palavras.
//
// ---- o que esta camada não faz ----
//
// Não modera. Revogar uma admissão **não** derruba quem está dentro e **não** é
// banir: é «pergunte-me outra vez». Derrubar e barrar têm verbos próprios e
// moram na caixa de moderação, e misturar as duas coisas aqui faria um ato
// brando ter consequência violenta — que é como uma interface ensina a não
// apertar nada.
//
// ---- toda classe daqui começa por `portaria-` ----
//
// As folhas caem num só espaço de nomes, na ordem em que `index.html` as
// carrega, e duas escolhendo o mesmo nome não é conflito que o navegador
// reporte: é a de baixo ganhando todo empate em silêncio.
// `no_two_screens_claim_the_same_class_name` guarda isso.

"use strict";

/** Para onde o teclado volta quando esta camada fecha. */
let focoAntesDaPortaria = null;

/**
 * Os degraus do ADR 0022 que significam «dá para chegar aqui de fora».
 *
 * Os nomes são os de `Degrau::nome()`, no `seele-server`, e são exatamente as
 * variantes em que `Degrau::alcanca_de_fora()` responde `true`. Não é
 * coincidência a manter à mão: `the_alarm_names_the_rungs_the_ladder_actually_reports`
 * lê as duas listas e reprova quando elas divergem.
 *
 * Nasceu divergente. A lista dizia `EnderecoGlobal`, `PortaAberta` e
 * `PontoDeEncontro`, e **nenhum desses três nomes existe** do lado Rust — o
 * alarme comparava contra invenções, nunca dava igual, e o vermelho de «este
 * Dogma está aberto e alcançável de fora» simplesmente não aparecia. Um alarme
 * que não dispara não é um alarme silencioso: é um alarme que ninguém sabe que
 * está quebrado.
 *
 * `SoRedeLocal` e `RedeLocalOuVpn` são os que não estão aqui, e a distinção é o
 * alarme inteiro: um Dogma aberto na rede de casa é o padrão que o ADR 0021
 * defende de propósito; o mesmo Dogma aberto com endereço alcançável da
 * internet é outra coisa, e é dessa que o vermelho é reservado.
 */
const ALCANCA_DE_FORA = [
  "PortaNoRoteador",
  "FuroDeNat",
  "Ipv6Direto",
  "EnderecoDireto",
];

/**
 * A impressão digital, quebrada em grupos para uma pessoa conferir.
 *
 * Sessenta e quatro caracteres seguidos não se comparam por olho — quem tenta
 * perde o lugar no meio e conclui que bate. Em grupos de oito, a conferência é
 * de oito pedaços curtos, que é a mesma razão de o ADR 0006 pôr a impressão do
 * Dogma num link em vez de mandar alguém ditá-la.
 */
function agrupar(impressao) {
  return (impressao.match(/.{1,8}/g) ?? [impressao]).join(" ");
}

/** Com que segredo a pessoa chegou, em palavras. */
function comoChegou(pedido) {
  if (pedido.segredo === "convite") {
    return pedido.observacao === ""
      ? "chegou com um convite de uso único"
      : `chegou com o convite «${pedido.observacao}»`;
  }
  if (pedido.segredo === "senha") return "chegou com a senha do Dogma";
  return "chegou sem segredo nenhum — o Dogma estava aberto";
}

/**
 * O cartão de um pedido.
 *
 * A ordem dos elementos **é** a decisão de desenho, e está no ADR 0030. A
 * impressão digital vem primeiro e por extenso porque é a identidade: é o que
 * se confere por outro canal, e é a única coisa aqui que outra pessoa não pode
 * escolher. O apelido vem abaixo, entre aspas e apresentado como afirmação —
 * *diz chamar-se* —, nunca como título do cartão. Título é do que a pessoa é, e
 * quem bateu ainda não é nada neste Dogma.
 *
 * É o mesmo corte que as `NOTAS-DE-RELEASE` fazem entre «o arquivo chegou
 * inteiro» e «este arquivo é bom»: uma coisa é o que se verificou, outra é o
 * juízo sobre ela.
 *
 * O ADR 0017 já impede pedir um apelido que é de outra chave. O que ele não
 * impede, e nenhum código impede, é o parecido — `Rafae1` ao lado de `Rafael`.
 * Contra isso não há verificação, só o hábito de ler a linha de cima; e é por
 * isso que a linha de cima é a de cima.
 */
function cartao(pedido, botoes) {
  const linha = elemento("li", "portaria-cartao");

  const impressao = elemento("p", "portaria-impressao", agrupar(pedido.impressao));
  impressao.title = pedido.impressao;

  const apelido = elemento("p", "portaria-apelido");
  apelido.append(
    elemento("span", "portaria-diz", "diz chamar-se"),
    elemento("span", "portaria-nome", `«${pedido.apelido}»`),
  );

  const contexto = elemento("p", "portaria-contexto", comoChegou(pedido));
  const batida = elemento(
    "p",
    "portaria-quando",
    pedido.batidas > 1
      ? `bateu ${quando(pedido.bateu_em)}, e ${pedido.batidas} vezes ao todo`
      : `bateu ${quando(pedido.bateu_em)}`,
  );

  const acoes = elemento("div", "portaria-acoes");
  acoes.append(...botoes);
  linha.append(impressao, apelido, contexto, batida, acoes);
  return linha;
}

/** Um botão desta camada. */
function botao(rotulo, aoApertar) {
  const alvo = elemento("button", "botao-fantasma", rotulo);
  alvo.type = "button";
  alvo.addEventListener("click", aoApertar);
  return alvo;
}

// ------------------------------------------------------------------ desenhar

/**
 * Lê o estado da porta e a fila, e redesenha tudo.
 *
 * Sob demanda, e não no tique de `atualizar()`. Duas razões, e a segunda é a
 * que importa: uma fila que se reconstrói duas vezes por segundo troca o botão
 * debaixo do dedo de quem ia apertar ADMITIR, e tira do documento o elemento
 * para onde a caixa de confirmação devolveria o foco — que é o defeito que
 * `opening_the_moderation_carries_the_keyboard_and_closing_gives_it_back`
 * descreve por inteiro.
 */
async function desenharPortaria() {
  const erro = $("portaria-erro");
  erro.hidden = true;

  let estado;
  let pedidos;
  try {
    estado = await invoke("estado_da_porta");
    pedidos = await invoke("pedidos_da_portaria");
  } catch (falha) {
    erro.hidden = false;
    erro.textContent = fraseDeErro(falha);
    return;
  }

  desenharCamadas(estado);
  desenharAlarme(estado);
  desenharFila(pedidos);
}

/** As três camadas, cada uma com o estado escrito. */
function desenharCamadas(estado) {
  $("portaria-estado-senha").textContent = estado.tem_senha
    ? "definida"
    : "nenhuma — a senha não fecha nada";
  $("portaria-estado-convites").textContent = estado.aceita_convites
    ? "há convites emitidos"
    : "nenhum convite emitido";

  const ligada = estado.portaria_ligada;
  $("portaria-estado-portaria").textContent = ligada
    ? "ligada — quem nunca entrou precisa da sua permissão"
    : "desligada — quem passa pelo segredo entra sem perguntar";

  const interruptor = $("portaria-ligar");
  interruptor.textContent = ligada ? "DESLIGAR" : "LIGAR";
  interruptor.setAttribute("aria-pressed", String(ligada));

  // O rótulo do botão que abre esta camada carrega o estado, porque é a única
  // coisa visível quando ela está fechada.
  $("portaria-abrir").textContent = rotuloDaPorta(estado);
}

/** O que o botão de abrir esta camada diz sobre a porta. */
function rotuloDaPorta(estado) {
  if (!estado.hospedando) return "porta";
  if (estado.pendentes > 0) return `PORTA · ${estado.pendentes} BATENDO`;
  if (estado.aberto && !estado.portaria_ligada) return "PORTA · ABERTA";
  return "PORTA · FECHADA";
}

/**
 * A banda de alerta, e o único vermelho desta camada.
 *
 * As duas condições juntas, e não uma: aberto na rede de casa é o padrão do ADR
 * 0021, defendido lá com um argumento que continua de pé. Alarmar nesse caso
 * gastaria o vermelho que existe para o outro — e um alarme que dispara no caso
 * normal é um alarme que se aprende a ignorar, que é o que o ADR 0003 diz sobre
 * o aviso de troca de chave.
 */
function desenharAlarme(estado) {
  const alarme = $("portaria-alarme");
  const escancarada = estado.aberto && !estado.portaria_ligada;
  const deFora = ALCANCA_DE_FORA.includes(estado.alcance);

  if (!escancarada || !deFora) {
    alarme.hidden = true;
    return;
  }
  alarme.hidden = false;
  alarme.textContent =
    "ESTE DOGMA ESTÁ ABERTO E ALCANÇÁVEL DE FORA DA SUA REDE.\n" +
    "Qualquer um que chegue ao endereço entra e ganha uma conta. " +
    "Ligue a portaria, ponha uma senha, ou as duas.";
}

/** A fila e o histórico. */
function desenharFila(pedidos) {
  const esperando = pedidos.filter((pedido) => pedido.decidido_em === null);
  const decididos = pedidos.filter((pedido) => pedido.decidido_em !== null);

  $("portaria-fila-vazia").hidden = esperando.length > 0;
  $("portaria-decididos-vazio").hidden = decididos.length > 0;

  // Ver a fila já é ter sido avisado. Sem isto a faixa voltaria no instante em
  // que esta camada fechasse, sobre exatamente a gente que acabou de ser lida —
  // e um aviso que reaparece sozinho depois de atendido é o que ensina a
  // dispensá-lo sem ler.
  calarBatidas(esperando.length);

  repovoar(
    $("portaria-fila"),
    esperando.map((pedido) =>
      cartao(pedido, [
        botao("ADMITIR", () => perguntarEDecidir(pedido, true)),
        botao("RECUSAR", () => perguntarEDecidir(pedido, false)),
      ]),
    ),
  );

  repovoar(
    $("portaria-decididos"),
    decididos.map((pedido) => {
      const cartaoFeito = cartao(pedido, [
        botao("REVOGAR", () => perguntarERevogar(pedido)),
      ]);
      cartaoFeito.prepend(
        elemento(
          "p",
          "portaria-veredito",
          pedido.admitido ? "ADMITIDO" : "RECUSADO",
        ),
      );
      return cartaoFeito;
    }),
  );
}

// --------------------------------------------------------------------- atos

/**
 * Admitir ou recusar, com a consequência escrita antes.
 *
 * Os dois passam pela caixa, e admitir também. Não é simetria decorativa: uma
 * admissão vale para sempre e não pergunta de novo — é a promessa inteira do
 * TOFU —, então é tão permanente quanto a recusa e merece a mesma frase.
 *
 * A caixa é a de `camada-moderar.js`, e é uma só de propósito. Uma segunda
 * forma de confirmar seria uma segunda chance de alguém escrever «tem certeza?»,
 * que é a confirmação que não informa nada.
 */
function perguntarEDecidir(pedido, admitir) {
  const nome = `«${pedido.apelido}»`;
  const consequencia = admitir
    ? `A chave que se diz ${nome} passa a entrar neste Dogma sem perguntar, ` +
      `agora e sempre, até você revogar.\n` +
      `Você está decidindo sobre a impressão digital, e não sobre o nome: ` +
      `${agrupar(pedido.impressao)}\n` +
      `Confira por outro canal antes, porque depois disto ninguém pergunta de novo.`
    : `A chave que se diz ${nome} não entra, e vai ler que quem hospeda recusou.\n` +
      `Não é banimento: você pode voltar atrás aqui mesmo, e ela pode bater de novo. ` +
      `Quem já está dentro não é derrubado por isto.`;

  abrirConfirmacao(
    admitir ? "ADMITIR QUEM BATEU" : "RECUSAR QUEM BATEU",
    consequencia,
    admitir ? "ADMITIR" : "RECUSAR",
    async () => {
      await invoke("decidir_pedido", { impressao: pedido.impressao, admitir });
      await desenharPortaria();
    },
  );
}

/**
 * Revogar, dizendo as duas coisas que a palavra não diz sozinha.
 *
 * Que não derruba quem está dentro, e que não é banir. Sem as duas, a frase
 * descreve um ato mais violento do que este é — e a caixa de moderação existe
 * justamente para a pessoa saber qual dos atos vai acontecer.
 */
function perguntarERevogar(pedido) {
  abrirConfirmacao(
    "REVOGAR A DECISÃO",
    `A chave que se diz «${pedido.apelido}» volta a ser desconhecida: ` +
      `na próxima vez que bater, você é perguntado de novo.\n` +
      `Isto não derruba ninguém que esteja dentro agora, e não é banir — ` +
      `banir é para sempre e mora na caixa de moderação.\n` +
      agrupar(pedido.impressao),
    "REVOGAR",
    async () => {
      await invoke("revogar_admissao", { impressao: pedido.impressao });
      await desenharPortaria();
    },
  );
}

/**
 * Tirar a senha escancara a porta, e por isso passa pela caixa.
 *
 * Pôr uma senha, gerar convite e ligar a portaria não passam: os três **fecham**
 * a porta, e obrigar uma confirmação para fechar é ensinar a apertar duas vezes
 * pelo ato que não machuca ninguém.
 */
function perguntarETirarSenha(estado) {
  const sobra = estado.portaria_ligada
    ? "A portaria continua ligada, então quem nunca entrou ainda precisa da sua permissão."
    : "Nada mais fecha esta porta: com a portaria desligada, qualquer um que chegue ao endereço entra.";
  abrirConfirmacao(
    "TIRAR A SENHA DO DOGMA",
    `Ninguém mais precisa saber a senha para chegar à porta deste Dogma.\n${sobra}`,
    "TIRAR",
    async () => {
      await invoke("definir_senha_do_dogma", { senha: null });
      await desenharPortaria();
    },
  );
}

/**
 * Desligar a portaria também escancara, e também passa pela caixa.
 *
 * As decisões já tomadas não somem — desligar é «pare de perguntar», não
 * «esqueça o que eu decidi» —, e a frase diz isso porque a palavra não diz.
 */
function perguntarEDesligar(estado) {
  const sobra = estado.tem_senha
    ? "A senha do Dogma continua valendo, então ainda é preciso sabê-la para chegar."
    : "Não há senha nem portaria: qualquer um que chegue ao endereço entra e ganha uma conta.";
  abrirConfirmacao(
    "DESLIGAR A PORTARIA",
    `Quem nunca entrou passa a entrar sem você ser perguntado.\n${sobra}\n` +
      `O que você já decidiu fica guardado, e volta a valer se você religar.`,
    "DESLIGAR",
    async () => {
      await invoke("ligar_portaria", { ligada: false });
      await desenharPortaria();
    },
  );
}

/** Roda um pedido que fecha a porta, e redesenha. Falha vira frase. */
async function fechando(acao) {
  const erro = $("portaria-erro");
  erro.hidden = true;
  try {
    await acao();
  } catch (falha) {
    erro.hidden = false;
    erro.textContent = fraseDeErro(falha);
    return;
  }
  await desenharPortaria();
}

// ------------------------------------------------------------ abrir e fechar

/**
 * Abre a camada, com o teclado dentro dela.
 *
 * WCAG 2.4.3, a mesma regra por que `abrirTela` existe: um diálogo que aparece
 * sem tomar o foco deixa quem navega por teclado apertando coisas que já não
 * estão na frente, e a um leitor de tela não aconteceu nada.
 */
async function abrirPortaria() {
  focoAntesDaPortaria = document.activeElement;
  $("portaria").hidden = false;
  $("portaria").focus();
  anunciar("A porta deste Dogma. Quem está batendo, e as três camadas de entrada.");
  await desenharPortaria();
}

/**
 * Fecha, devolvendo o teclado a quem abriu.
 *
 * `focavel` antes de `focus()`: o botão que abriu vive na linha do convite, que
 * pode ter sido escondida enquanto a camada estava aberta — e `focus()` num nó
 * fora do documento não faz nada e não reporta nada, que é o defeito original
 * reintroduzido por dentro.
 */
function fecharPortaria() {
  $("portaria").hidden = true;
  if (focavel(focoAntesDaPortaria)) {
    focoAntesDaPortaria.focus();
  } else {
    // `devolverFocoDaFaixa` e não `$("portaria-abrir").focus()` direto: a camada
    // passou a ser aberta também pela faixa de quem está batendo, que aparece
    // sobre qualquer tela — e quem hospeda de dentro de uma jaula tem o botão da
    // porta escondido junto com a sessão. `focus()` nele não faria nada e não
    // reportaria nada, que é o defeito original de volta.
    devolverFocoDaFaixa();
  }
  focoAntesDaPortaria = null;
}

// ------------------------------------------------------------------ ouvintes

/**
 * Uma leitura da porta, e as duas coisas que ela move.
 *
 * A resposta forte à pergunta «e se ninguém estiver olhando?» é do servidor: o
 * pedido é uma linha em SQLite e sobrevive à janela minimizada, ao app fechado e
 * à máquina reiniciada, então nada se perde por ninguém olhar. Estas duas são as
 * respostas baratas, e são complementares.
 *
 * O **rótulo do botão** carrega o estado da porta e o número de quem espera —
 * é o que se lê sem procurar, para quem está olhando a sessão. A **faixa**
 * chama, e chama de qualquer tela: ver `avisarQueBatem` logo abaixo, e o motivo
 * de ela existir apesar do rótulo.
 *
 * Cinco segundos, e não o quarto de segundo de `atualizar()`: ninguém bate à
 * porta quatro vezes por segundo, e uma consulta ao banco do Dogma por quadro
 * seria pagar caro por um número que muda uma vez por dia. Quando não se está
 * hospedando, `estado_da_porta` responde `hospedando: false` na hora, o rótulo
 * volta a ser a palavra sem estado e a faixa some.
 *
 * O toque no ombro com a **janela fechada** — uma notificação do sistema — não
 * está aqui: ela é do `tauri-plugin-notification`, que não está nas
 * dependências. É o que sobrou da pendência 23, item 1.
 */
async function atualizarPorta() {
  try {
    const estado = await invoke("estado_da_porta");
    $("portaria-abrir").textContent = rotuloDaPorta(estado);
    avisarQueBatem(estado.hospedando ? estado.pendentes : 0);
  } catch (falha) {
    console.warn("estado_da_porta:", falha);
  }
}

// -------------------------------------------------- a faixa de quem está batendo
//
// A pendência 23, item 1, na metade que não precisa de dependência nova: o
// pedido durava e nada *chamava*. O chip acima já contava — e ele mora dentro
// de `#tela-sessao`, então some inteiro quando quem hospeda entra numa jaula ou
// abre o Terminal Dogma, que é justo a hora em que ninguém está olhando para
// ele. A faixa fica fora das telas e sobrevive a todas.
//
// O que ela **não** faz está escrito ao lado da marcação e vale repetir aqui,
// porque é a parte que um conserto distraído desfaz: ela nunca chama `focus()`.
// Quem hospeda pode estar falando numa jaula, e o push-to-talk depende de o
// foco não estar num campo de texto. Um aviso que interrompe uma conversa é
// pior que o silêncio que havia antes dele.

/**
 * Acima de quantos pendentes a faixa volta a aparecer.
 *
 * Zero quando ninguém está esperando. Sobe para o tamanho da fila quando quem
 * hospeda a lê ou aperta DEPOIS — e é isso que faz DEPOIS calar **estas**
 * batidas sem calar a próxima, que é a única promessa que um aviso dispensável
 * pode fazer.
 */
let batidasCaladas = 0;

/** O último número lido, que é o que DEPOIS precisa saber para calar. */
let batidasAgora = 0;

/**
 * Mostra, esconde e — uma vez por aparição — diz em voz alta que há gente
 * esperando.
 *
 * A frase é dita por `anunciar`, a região viva que a janela já tem, e não por
 * um `aria-live` nesta faixa: duas regiões vivas com a mesma frase são a mesma
 * frase lida duas vezes. Ela sai **uma vez por aparição**, e não a cada leitura
 * de cinco segundos — repetir a mesma notícia doze vezes por minuto é como se
 * ensina alguém a não ouvir a décima terceira.
 */
function avisarQueBatem(pendentes) {
  batidasAgora = pendentes;
  const faixa = $("portaria-batendo");

  if (pendentes === 0) {
    // Fila vazia zera a memória: quem bater depois disto é batida nova, e não a
    // continuação de uma que já foi dispensada.
    batidasCaladas = 0;
    faixa.hidden = true;
    return;
  }

  const frase =
    pendentes === 1
      ? "ALGUÉM ESTÁ BATENDO À PORTA DESTE DOGMA"
      : `${pendentes} PESSOAS ESTÃO BATENDO À PORTA DESTE DOGMA`;
  $("portaria-batendo-texto").textContent = frase;

  const mostrar = pendentes > batidasCaladas;
  // Antes de escrever o `hidden`, porque a pergunta é se ela **estava**
  // escondida: é a transição que merece uma frase, não o estado.
  if (mostrar && faixa.hidden) {
    anunciar(`${frase}. Abra a porta do Dogma para decidir.`);
  }
  faixa.hidden = !mostrar;
}

/** Cala a faixa para uma fila deste tamanho. */
function calarBatidas(pendentes) {
  batidasCaladas = pendentes;
  batidasAgora = pendentes;
  $("portaria-batendo").hidden = true;
}

/**
 * Para onde o teclado vai quando a faixa se fecha debaixo dele.
 *
 * A faixa nunca toma o foco — mas quem aperta DEPOIS está com o foco dentro
 * dela, e esconder o ancestral do elemento focado devolve o foco ao `<body>`.
 * É o mesmo defeito que `abrirTela` existe para consertar, entrando pela porta
 * dos fundos.
 *
 * O botão da porta primeiro, que é o que continua sendo sobre este assunto; e,
 * quando ele não está na frente — quem hospeda numa jaula não vê aquela linha
 * —, a tela que estiver aberta, que carrega `tabindex="-1"` justamente para
 * poder receber o foco nesse caso.
 */
function devolverFocoDaFaixa() {
  const abrir = $("portaria-abrir");
  if (focavel(abrir)) {
    abrir.focus();
    return;
  }
  const tela = document.querySelector(".tela:not([hidden])");
  if (tela) tela.focus();
}

const PORTA_A_CADA_MS = 5000;
setInterval(() => {
  atualizarPorta().catch((falha) => console.warn("porta:", falha));
}, PORTA_A_CADA_MS);

$("portaria-abrir").addEventListener("click", () => {
  abrirPortaria().catch((falha) => console.warn("portaria:", falha));
});

// Um clique da faixa até a decisão. `abrirPortaria` leva o teclado para dentro
// da camada, e `fecharPortaria` o devolve — inclusive quando o botão que o
// mandou já não está no documento, que é o caso desta faixa.
$("portaria-batendo-ver").addEventListener("click", () => {
  abrirPortaria().catch((falha) => console.warn("portaria:", falha));
});

$("portaria-batendo-depois").addEventListener("click", () => {
  calarBatidas(batidasAgora);
  devolverFocoDaFaixa();
});

$("portaria-fechar").addEventListener("click", fecharPortaria);

$("portaria").addEventListener("keydown", (evento) => {
  if (evento.key === "Escape") fecharPortaria();
});

$("portaria-por-senha").addEventListener("click", () => {
  const campo = $("portaria-senha");
  const senha = campo.value;
  if (senha === "") return;
  fechando(async () => {
    await invoke("definir_senha_do_dogma", { senha });
    campo.value = "";
  });
});

$("portaria-tirar-senha").addEventListener("click", () => {
  invoke("estado_da_porta")
    .then(perguntarETirarSenha)
    .catch((falha) => console.warn("portaria:", falha));
});

$("portaria-gerar").addEventListener("click", () => {
  const observacao = $("portaria-observacao");
  fechando(async () => {
    // O link inteiro cai no campo de convite que já existe, em vez de numa
    // caixa nova: é de lá que quem hospeda já copia, e um segundo lugar para
    // copiar um link é um lugar a mais para copiar o link errado.
    const link = await invoke("criar_convite_do_dogma", {
      observacao: observacao.value.trim(),
    });
    $("convite-link").value = link;
    observacao.value = "";
    anunciar("Convite gerado. O link no campo CONVITE agora leva este convite.");
  });
});

$("portaria-ligar").addEventListener("click", () => {
  const ligada = $("portaria-ligar").getAttribute("aria-pressed") === "true";
  if (ligada) {
    invoke("estado_da_porta")
      .then(perguntarEDesligar)
      .catch((falha) => console.warn("portaria:", falha));
    return;
  }
  fechando(() => invoke("ligar_portaria", { ligada: true }));
});
