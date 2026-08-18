// SEELE · Entry Plug — o Terminal Dogma (`#tela-dogma`).
//
// A configuração local: o que é desta máquina e não deste Dogma. Quatro seções —
// ÁUDIO, ATALHOS, IDENTIDADE e ATUALIZAÇÃO. As três primeiras são a forma do
// comp v3, §8; a última é o botão de atualizar do ADR 0026, que é posterior ao
// comp e cai aqui porque qual SEELE está instalado é a coisa mais **desta
// máquina** que existe.
//
// APARÊNCIA saiu. Ela era o comp inteiro numa chave só — LEGENDAS SIMPLES —, e
// aquela camada deixou de ser um modo: a nota ao lado de um controle é parte do
// controle e aparece sempre. Uma seção de configuração sem nada para configurar
// é uma promessa de que há o que ajustar ali, e não havia.
//
// Alcançável das duas telas vivas, e volta para a que a abriu. Escolher
// microfone antes de conectar é tão comum quanto durante, e uma configuração
// atrás da sessão poria o controle atrás da porta que ele serve para abrir.
//
// ---- não há SALVAR, e a ausência é a decisão ----
//
// O comp desenha `SALVAR`, `DESCARTAR` e um estado sujo. Nada disso está aqui.
// A escolha vale na hora, e o que a tela mostra é o que **está valendo** — a
// lista de microfones diz qual abriu, e não qual foi pedido. Confirmação por
// estado, não por botão: um `SALVAR` num painel de áudio promete que nada muda
// até apertá-lo, e isso é falso para som, porque você precisa ouvir o efeito
// para saber se escolheu certo.
//
// ---- o que este arquivo não decide ----
//
// Nada. A lista de dispositivos vem do Rust, a escolha vai para o Rust, e é lá
// que ela é gravada e aplicada. Este arquivo não sabe o que é um id de
// dispositivo: ele o desenha nunca e o devolve inteiro. O modo de voz é o mesmo
// acordo — os três nomes chegam e voltam como chegaram, e nada aqui sabe o que
// cada um faz com o microfone.

"use strict";

/** Para onde VOLTAR devolve. `null` enquanto esta tela está fechada. */
let telaDeOrigem = null;

/**
 * O id escolhido, ou `null` para o padrão da máquina.
 *
 * Guardado aqui só para desenhar qual linha está acesa entre uma leitura e a
 * seguinte. A verdade mora no disco, do lado do Rust; isto é uma cópia da
 * última resposta dele, e toda escolha o consulta de novo.
 */
const escolhidoDe = { captura: null, saida: null };

/**
 * Os dois lados do áudio, e como cada um se pergunta ao Rust.
 *
 * Uma tabela e não duas cópias do mesmo desenho. Entrada e saída se escolhem
 * exatamente do mesmo jeito — listar, ler o escolhido, escolher, e mostrar o
 * que **abriu** e não o que foi pedido — e duas implementações paralelas dessa
 * mesma dança divergiriam na primeira correção que alguém fizesse só de um
 * lado. O `seele-audio` fez a mesma escolha na camada dele, com `Wanted` e
 * `DeviceChoice` carregando os dois lados juntos em vez de dois `Option` em
 * fila, e pelo mesmo motivo.
 *
 * Os comandos entram como chamadas escritas, e não como nomes em variável, e
 * isso não é estilo. `tests/frontend.rs` amarra cada `invoke("…")` a um comando
 * registrado em `main.rs`, nos dois sentidos, procurando o literal no texto —
 * um nome guardado numa variável some desse laço, e o guarda passa a dizer que
 * seis comandos vivos nunca são chamados. A primeira versão desta tabela fez
 * exatamente isso.
 */
const LADOS = [
  {
    chave: "captura",
    lista: "lista-microfones",
    listar: () => invoke("microfones"),
    lerEscolhido: () => invoke("microfone_escolhido"),
    escolher: (dispositivo) => invoke("escolher_microfone", { dispositivo }),
  },
  {
    chave: "saida",
    lista: "lista-saidas",
    listar: () => invoke("saidas"),
    lerEscolhido: () => invoke("saida_escolhida"),
    escolher: (dispositivo) => invoke("escolher_saida", { dispositivo }),
  },
];

/** Quantos blocos o medidor de entrada tem. 26, como no comp. */
const BLOCOS_DO_MEDIDOR = 26;

/**
 * O que a porta de saída diz, conforme de onde se entrou.
 *
 * O comp escreve `VOLTAR AO DOGMA` e daqui isso só é verdade metade das vezes:
 * esta tela também abre da entrada, onde não há Dogma nenhum para voltar. Um
 * botão que promete o lugar errado é pior que um botão genérico.
 */
const VOLTA = {
  "tela-boot": "VOLTAR À ENTRADA",
  "tela-sessao": "VOLTAR AO DOGMA",
};

// ------------------------------------------------------------------- seções

/**
 * Abre uma seção e fecha as outras.
 *
 * O título e o subtítulo saem do `data-titulo` e do `data-sub` do próprio
 * botão. Uma tabela em JavaScript com as mesmas oito frases seria uma segunda
 * lista para alguém esquecer de atualizar no dia em que uma seção mudar de
 * nome — e o comp já erra assim, com `SECOES` longe da marcação que a desenha.
 *
 * `aria-current` e não só a barra laranja: a barra é o que sobra em
 * monocromático, e o `aria-current` é o que atravessa para quem não vê nenhuma
 * das duas.
 */
function abrirSecao(id) {
  for (const botao of document.querySelectorAll(".dogma-secao")) {
    const atual = botao.id === id;
    botao.setAttribute("aria-current", atual ? "true" : "false");
    $(botao.dataset.painel).hidden = !atual;
    if (!atual) continue;
    $("dogma-titulo").textContent = botao.dataset.titulo;
    $("dogma-subtitulo").textContent = botao.dataset.sub;
  }
}

// ------------------------------------------------------------------- desenho

/**
 * A lista de microfones, com o escolhido aceso.
 *
 * A linha de cima é sempre o padrão da máquina. Não é um dispositivo: é a
 * ausência de escolha, e ela precisa ser escolhível de volta — sem ela, quem
 * experimentou uma interface e a desconectou ficaria com uma preferência
 * apontando para um aparelho que não existe mais e sem nada na tela que a
 * desfaça.
 */
async function desenharDispositivos() {
  await Promise.all(LADOS.map(desenharUmLado));
}

async function desenharUmLado(lado) {
  const [dispositivos, escolhido] = await Promise.all([
    lado.listar(),
    lado.lerEscolhido(),
  ]);
  escolhidoDe[lado.chave] = escolhido ?? null;

  const lista = $(lado.lista);
  if (dispositivos.length === 0) {
    // Lista vazia é "a máquina não quis enumerar", e não "não há aparelho".
    // Quem escreve a segunda frase aqui mente para quem tem áudio funcionando.
    const vazio = elemento("li", "dogma-dispositivos-vazio", "ESTA MÁQUINA NÃO LISTOU DISPOSITIVO NENHUM");
    repovoar(lista, [vazio]);
    return;
  }

  const linhas = [linhaDeDispositivo(lado, "", "PADRÃO DA MÁQUINA", false)];
  for (const dispositivo of dispositivos) {
    linhas.push(linhaDeDispositivo(lado, dispositivo.id, dispositivo.name, dispositivo.default));
  }
  repovoar(lista, linhas);
  // Marcadas na mesma tarefa em que nascem. Deixar para o snapshot seguinte
  // deixaria um quadro com a lista inteira apagada, e o quadro apagado é o que
  // diz "nenhum destes está escolhido".
  marcarLinhas(null);
}

/**
 * Uma linha da lista, de qualquer um dos dois lados. `id` vazio é o padrão da
 * máquina, que não é dispositivo nenhum: é a ausência de escolha, e precisa ser
 * escolhível de volta — sem ela, quem experimentou uma interface e a
 * desconectou ficaria com uma preferência apontando para um aparelho que não
 * existe mais e sem nada na tela que a desfaça.
 */
function linhaDeDispositivo(lado, id, nome, ehPadrao) {
  const linha = elemento("li");
  const botao = elemento("button", "dogma-dispositivo");
  botao.type = "button";
  botao.dataset.dispositivo = id;
  botao.dataset.padrao = ehPadrao ? "sim" : "nao";
  botao.append(
    elemento("span", "dogma-dispositivo-nome", nome),
    elemento("span", "dogma-dispositivo-marca"),
  );
  // O id sai daqui exatamente como entrou. Nada nesta janela o interpreta —
  // vazio vira `null`, que é como o Rust escreve "o padrão da máquina".
  botao.addEventListener("click", () => escolher(lado, id === "" ? null : id));

  linha.append(botao);
  return linha;
}

/**
 * Acende o que está escolhido e diz o que está aberto.
 *
 * São duas coisas e não uma, e é a divergência entre elas que esta tela existe
 * para mostrar: o escolhido é o que ficou gravado, e o aberto é o que a máquina
 * conseguiu abrir. Empatam na maior parte do tempo e separam justamente quando
 * importa — a interface escolhida ontem que está desconectada hoje aparece como
 * ESCOLHIDO, e o microfone embutido que assumiu o lugar dela como EM USO. Uma
 * tela que desenhasse só o primeiro chamaria a escolha de realidade.
 *
 * É também o que substitui o `SALVAR` do comp: não há o que confirmar quando a
 * tela já diz o que está valendo.
 *
 * A marca é texto, e não só a barra laranja: `specs/05-cliente-tui.md` proíbe
 * informação transmitida só por cor.
 */
function marcarLinhas(snapshot) {
  // Os dois campos são lidos aqui, e não na tabela `LADOS`, porque são a coisa
  // que esta função existe para comparar: `capture` e `playback` são o que o
  // `Snapshot` diz ter **aberto**, contra o que ficou gravado.
  marcarUmaLista("captura", $("lista-microfones"), snapshot?.capture?.id ?? null);
  marcarUmaLista("saida", $("lista-saidas"), snapshot?.playback?.id ?? null);
}

function marcarUmaLista(chave, lista, aberto) {
  for (const botao of lista.querySelectorAll(".dogma-dispositivo")) {
    const id = botao.dataset.dispositivo === "" ? null : botao.dataset.dispositivo;
    const escolhido = escolhidoDe[chave] === id;
    botao.dataset.escolhido = escolhido ? "sim" : "nao";

    let marca = "";
    if (aberto !== null && aberto === id) marca = "EM USO";
    else if (escolhido) marca = "ESCOLHIDO";
    else if (botao.dataset.padrao === "sim") marca = "PADRÃO";

    const alvo = botao.querySelector(".dogma-dispositivo-marca");
    if (alvo) alvo.textContent = marca;
  }
}

/**
 * O medidor de entrada, do mesmo `input_level` que a telemetria da sessão lê.
 *
 * Sem sessão não há nível. Um medidor parado em zero diria "seu microfone não
 * capta nada", que é uma frase diferente e falsa — daí o travessão.
 */
function desenharNivel(snapshot) {
  const medidor = $("dogma-nivel");
  if (!snapshot || !snapshot.audio_available) {
    medidor.dataset.vivo = "nao";
    medidor.textContent = "— SEM SESSÃO DE ÁUDIO";
    return;
  }

  const nivel = Math.max(0, Math.min(1, snapshot.telemetry.input_level));
  const cheios = Math.round(nivel * BLOCOS_DO_MEDIDOR);
  medidor.dataset.vivo = "sim";
  // O número acompanha os blocos pela mesma regra que a marca acompanha a cor:
  // a forma sozinha não é legível em monocromático nem por leitor de tela.
  medidor.textContent =
    "█".repeat(cheios) + "░".repeat(BLOCOS_DO_MEDIDOR - cheios) + ` ${Math.round(nivel * 100)}%`;
}

/**
 * Qual dos três modos do microfone está valendo.
 *
 * Três e não a chave de sim ou não do comp. `specs/03-audio.md` faz da tecla o
 * padrão porque ela nunca dispara sozinha, e uma chave de dois lados esconderia
 * o terceiro estado — o aberto, que é justamente o que ninguém quer ligar sem
 * saber. `aria-pressed` carrega a escolha para quem não vê o preenchimento.
 *
 * Sem sessão os três ficam apagados: `set_voice_mode` fala com um plug inserido,
 * e não há preferência em disco que os lembre. Apagado **e** dizendo por quê, ou
 * a lacuna se lê como defeito.
 */
function desenharModos(snapshot) {
  const semSessao = !snapshot;
  for (const botao of document.querySelectorAll(".dogma-modo")) {
    botao.disabled = semSessao;
    botao.setAttribute("aria-pressed", !semSessao && snapshot.voice_mode === botao.dataset.modo);
    if (semSessao) {
      botao.title = "Como o microfone abre é da sessão: sem plug inserido não há microfone para abrir.";
    } else {
      botao.removeAttribute("title");
    }
  }
}

/**
 * O apelido em uso.
 *
 * O único dado de identidade que esta janela alcança sem um comando novo. A
 * chave em si está em disco (ADR 0017) e nada a lê para cá — daí não haver tipo,
 * impressão digital nem data nesta seção, e nem molduras vazias no lugar delas.
 *
 * Sem sessão não há apelido reivindicado, e o travessão é a resposta certa: o
 * nome que se digita na entrada só vira o seu depois que o CASPER o vincula.
 */
function desenharIdentidade(snapshot) {
  const alvo = $("dogma-apelido");
  const apelido = snapshot?.nickname ?? "";
  alvo.textContent = apelido === "" ? "——" : apelido;
  alvo.classList.toggle("ausente", apelido === "");
}

// --------------------------------------------------------------------- ações

/** Escolhe um microfone, ou volta para o padrão da máquina com `null`. */
async function escolher(lado, id) {
  const erro = $("dogma-erro");
  erro.hidden = true;
  try {
    await lado.escolher(id);
  } catch (falha) {
    // Revelar antes de escrever: `role="alert"` não anuncia o que já estava na
    // página enquanto ela estava escondida.
    erro.hidden = false;
    erro.textContent = fraseDeErro(falha);
  }
  await desenharDispositivos();
  await atualizarDogma();
}

/** Troca o modo do microfone. O nome do modo volta como chegou. */
async function escolherModo(modo) {
  try {
    await invoke("set_voice_mode", { mode: modo });
  } catch (falha) {
    console.warn("set_voice_mode:", falha);
  }
  await atualizarDogma();
}

/** Abre a configuração, lembrando de onde. */
async function abrirDogma(origem) {
  // Com a tela de origem ainda visível, ou não há foco a lembrar: é o que
  // devolve a engrenagem — ou o TERMINAL DOGMA da entrada — a quem a apertou.
  guardarFoco(origem);
  telaDeOrigem = origem;
  $(origem).hidden = true;
  $("tela-dogma").hidden = false;
  $("dogma-erro").hidden = true;
  $("dogma-voltar-texto").textContent = VOLTA[origem] ?? "VOLTAR";
  await desenharDispositivos();
  await atualizarDogma();
  abrirTela("tela-dogma");
}

/** Fecha e devolve para a tela que a abriu. */
function fecharDogma() {
  guardarFoco("tela-dogma");
  $("tela-dogma").hidden = true;
  const volta = telaDeOrigem ?? "tela-boot";
  $(volta).hidden = false;
  telaDeOrigem = null;
  // A mesma porta, dos dois lados: quem entrou pela engrenagem da sessão sai
  // nela, e quem entrou pelo TERMINAL DOGMA da entrada sai nele.
  voltarParaTela(volta);
  // Só quando se volta para a sessão: `desenharMensagens` sai cedo enquanto a
  // lista está sem layout, e redesenhar aqui é o outro lado desse acordo. Vindo
  // da tela de entrada não há sessão nenhuma a redesenhar.
  if (volta === "tela-sessao") {
    atualizar().catch((falha) => console.warn("voltar do Terminal Dogma:", falha));
  }
}

/**
 * Some sem devolver ninguém.
 *
 * Para quem já está trocando de tela por cima: a sessão pode acabar enquanto
 * esta tela está aberta — um operador derruba, o enlace descarrega —, e
 * `mostrarFim` escolhe a tela seguinte por conta própria. Fechar de volta para a
 * origem ali reabriria a sessão que acabou de terminar; não fechar deixaria duas
 * telas empilhadas, porque toda `.tela` tem a altura da janela.
 */
function abandonarDogma() {
  $("tela-dogma").hidden = true;
  telaDeOrigem = null;
}

/**
 * Puxa o snapshot para o medidor, para quem está capturando, para os modos do
 * microfone e para o apelido.
 *
 * Sem sessão isto falha, e falhar é o estado normal desta tela quando aberta da
 * entrada — não é aviso de nada.
 */
async function atualizarDogma() {
  let snapshot = null;
  try {
    snapshot = await invoke("snapshot");
  } catch (falha) {
    if (falha !== "NotConnected") console.warn("snapshot:", falha);
  }
  desenharNivel(snapshot);
  marcarLinhas(snapshot);
  desenharModos(snapshot);
  desenharIdentidade(snapshot);
}

// --------------------------------------------------------------- atualizar
//
// ADR 0026, e o desenho é a decisão daquele ADR: **quem decide é a pessoa**.
// Duas metades — procurar só olha, instalar só instala o que já foi olhado — e
// nenhuma das duas roda sozinha. Não há consulta ao abrir a janela, não há
// consulta no laço de meio segundo desta tela, e não há instalação silenciosa.
// Num produto cujo argumento é que o servidor é seu, um app que fala com o
// github.com a cada arranque contradiz o argumento.
//
// O que este arquivo **não** decide: se há versão nova, se o pacote é
// legítimo, e o que fazer quando não é. Tudo isso é do Rust, e chega aqui como
// `Option<VersaoNova>` ou como uma das seis variantes de `FalhaAoAtualizar`,
// que viram frase em `frases.js` como todo o resto.

/** Quantos blocos a barra do download tem. Vinte, como a do roster. */
const BLOCOS_DO_DOWNLOAD = 20;

/** Quantos bytes tem um megabyte, para a contagem ao lado da barra. */
const BYTES_POR_MEGA = 1024 * 1024;

/** `4,2 MB` — o tamanho como uma pessoa o lê, no idioma da máquina. */
function emMegabytes(bytes) {
  const mega = bytes / BYTES_POR_MEGA;
  return `${mega.toLocaleString(undefined, {
    minimumFractionDigits: 1,
    maximumFractionDigits: 1,
  })} MB`;
}

/**
 * O andamento do download, do canal `seele://atualizacao`.
 *
 * `total` é opcional porque o servidor pode não mandar o tamanho, e é aí que
 * está a única decisão desta função: **sem total não há barra**. Uma barra
 * desenhada sobre um denominador inventado trava em algum lugar e mente sobre
 * quanto falta; o travessão e a contagem ao lado dizem a verdade inteira — já
 * vieram tantos megabytes, e ninguém disse de quantos.
 *
 * É a mesma resposta que a barra da bateria interna dá para a mesma falta, e
 * ela usa o mesmo `naoMedido` de `tela-sessao.js` para dizê-lo.
 */
function desenharAndamentoDoDownload(andamento) {
  $("atualizacao-andamento").hidden = false;
  const barra = $("atualizacao-barra");
  const contagem = $("atualizacao-baixados");

  if (!andamento.total) {
    naoMedido(
      barra,
      "o servidor não disse de quantos bytes é o pacote: dá para dizer quanto já veio, nunca quanto falta",
    );
    contagem.textContent = `${emMegabytes(andamento.baixados)} até agora`;
    return;
  }

  const parte = Math.max(0, Math.min(100, Math.round((andamento.baixados / andamento.total) * 100)));
  // A porcentagem ao lado dos blocos, sempre: `specs/05-cliente-tui.md` proíbe
  // informação que só a forma carregue, e uma parede de blocos é forma.
  medido(barra, `${blocos(parte, BLOCOS_DO_DOWNLOAD)} ${parte}%`);
  contagem.textContent = `${emMegabytes(andamento.baixados)} de ${emMegabytes(andamento.total)}`;
}

/**
 * Escreve o que a consulta achou — ou que não achou nada.
 *
 * Não achar nada é a resposta boa e comum, e ela tem frase própria: «você está
 * na última versão» é uma coisa diferente de «não consegui perguntar», e as
 * duas caindo no mesmo lugar da tela seriam a mesma tela para um sucesso e uma
 * falha.
 */
function desenharAchado(nova) {
  const achado = $("atualizacao-achado");
  if (!nova) {
    achado.hidden = true;
    $("atualizacao-estado").textContent = "VOCÊ ESTÁ NA ÚLTIMA VERSÃO.";
    return;
  }

  // «da X para a Y», e não uma seta: a face de dados embarcada não tem seta, e
  // as duas versões escritas por extenso são o que uma pessoa compara.
  $("atualizacao-estado").textContent =
    `HÁ VERSÃO NOVA: da ${nova.instalada} para a ${nova.versao}.`;

  // As notas são opcionais — o manifesto pode não trazer nenhuma. O bloco some
  // inteiro nesse caso, porque um cabeçalho «O QUE MUDA» sobre o vazio é pior
  // que nenhum cabeçalho.
  const notas = (nova.notas ?? "").trim();
  $("atualizacao-notas-bloco").hidden = notas === "";
  $("atualizacao-notas").textContent = notas;

  achado.hidden = false;
}

/** Pergunta se há versão nova. Não baixa nada. */
async function procurarAtualizacao() {
  const botao = $("atualizacao-procurar");
  const erro = $("atualizacao-erro");
  botao.disabled = true;
  erro.hidden = true;
  $("atualizacao-estado").textContent = "PROCURANDO…";
  try {
    desenharAchado(await invoke("procurar_atualizacao"));
  } catch (falha) {
    $("atualizacao-estado").textContent = "";
    $("atualizacao-achado").hidden = true;
    // Revelado antes de escrito, como todo `role="alert"` desta janela.
    erro.hidden = false;
    erro.textContent = fraseDeErro(falha);
  } finally {
    botao.disabled = false;
  }
}

/**
 * Baixa, confere a assinatura, instala e reabre.
 *
 * **Não há caminho de sucesso depois do `await`.** Quando dá certo, o processo
 * é encerrado e reaberto — no Windows pelo próprio instalador, nos outros dois
 * por `app.restart()` —, então a única linha que roda depois é a do erro. O
 * aviso do que isso custa está na marcação, acima do botão, e é lido antes de
 * ele ser apertado.
 */
async function instalarAtualizacao() {
  const instalar = $("atualizacao-instalar");
  const procurar = $("atualizacao-procurar");
  const erro = $("atualizacao-erro");
  instalar.disabled = true;
  procurar.disabled = true;
  erro.hidden = true;
  $("atualizacao-estado").textContent =
    "BAIXANDO E CONFERINDO A ASSINATURA. O SEELE VAI FECHAR E ABRIR DE NOVO SOZINHO.";
  $("atualizacao-andamento").hidden = false;
  naoMedido($("atualizacao-barra"), "o download ainda não relatou nada");
  $("atualizacao-baixados").textContent = "";

  try {
    await invoke("instalar_atualizacao");
  } catch (falha) {
    // Qualquer falha daqui deixa este SEELE inteiro: o pacote é conferido antes
    // de qualquer arquivo instalado ser tocado. As seis frases dizem isso.
    $("atualizacao-andamento").hidden = true;
    $("atualizacao-estado").textContent = "";
    erro.hidden = false;
    erro.textContent = fraseDeErro(falha);
    instalar.disabled = false;
    procurar.disabled = false;
    // A escolha foi consumida do lado do Rust — `instalar_atualizacao` a tira
    // do lugar em vez de emprestá-la. Quem quiser tentar de novo procura de
    // novo, e a consulta nova é o que confirma que a versão ainda é aquela.
    $("atualizacao-achado").hidden = true;
  }
}

// ------------------------------------------------------------------- ligação

$("botao-dogma").addEventListener("click", () => abrirDogma("tela-boot"));
$("botao-dogma-sessao").addEventListener("click", () => abrirDogma("tela-sessao"));
$("dogma-fechar").addEventListener("click", fecharDogma);

for (const botao of document.querySelectorAll(".dogma-secao")) {
  botao.addEventListener("click", () => abrirSecao(botao.id));
}

for (const botao of document.querySelectorAll(".dogma-modo")) {
  botao.addEventListener("click", () => escolherModo(botao.dataset.modo));
}

$("atualizacao-procurar").addEventListener("click", procurarAtualizacao);
$("atualizacao-instalar").addEventListener("click", instalarAtualizacao);

// O andamento do download. Um canal separado do `seele://event` da conversa, e
// a separação é do Rust: aquele carrega o que a FFI emite sobre a sessão, e
// bytes baixados não são evento de sessão nenhuma — quem baixa não precisa
// estar em sessão, e quem está em sessão não deve peneirar bytes.
listen("seele://atualizacao", (evento) => {
  desenharAndamentoDoDownload(evento.payload);
});

// Escape fecha, que é o que uma tela sobreposta faz. Só com ela na frente, ou
// engoliria a tecla de quem está fechando uma busca na sessão.
window.addEventListener("keydown", (evento) => {
  if (evento.key === "Escape" && !$("tela-dogma").hidden) {
    evento.preventDefault();
    fecharDogma();
  }
});

// O estado inicial da tela, escrito uma vez com ela ainda escondida: a seção de
// áudio aberta. Nenhum quadro chega a mostrar o cabeçalho vazio.
abrirSecao("secao-audio");

// O nível de entrada muda sozinho, e é a única coisa viva nesta tela. Mesmo
// meio segundo da telemetria da sessão, e só com a tela na frente: um `invoke`
// duas vezes por segundo para uma tela escondida é uma volta de IPC por nada.
setInterval(() => {
  if (!$("tela-dogma").hidden) atualizarDogma();
}, 500);
