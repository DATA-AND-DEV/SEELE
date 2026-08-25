// SEELE — o Terminal server (`#tela-server`).
//
// A configuração local: o que é desta máquina e não deste servidor. Quatro seções —
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
 * O comp escreve `VOLTAR AO SERVIDOR` e daqui isso só é verdade metade das vezes:
 * esta tela também abre da entrada, onde não há servidor nenhum para voltar. Um
 * botão que promete o lugar errado é pior que um botão genérico.
 */
const VOLTA = {
  "tela-boot": "VOLTAR À ENTRADA",
  "tela-sessao": "VOLTAR AO SERVIDOR",
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
  for (const botao of document.querySelectorAll(".server-secao")) {
    const atual = botao.id === id;
    botao.setAttribute("aria-current", atual ? "true" : "false");
    $(botao.dataset.painel).hidden = !atual;
    if (!atual) continue;
    $("server-titulo").textContent = botao.dataset.titulo;
    $("server-subtitulo").textContent = botao.dataset.sub;
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
    const vazio = elemento("li", "server-dispositivos-vazio", "ESTA MÁQUINA NÃO LISTOU DISPOSITIVO NENHUM");
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
  const botao = elemento("button", "server-dispositivo");
  botao.type = "button";
  botao.dataset.dispositivo = id;
  botao.dataset.padrao = ehPadrao ? "sim" : "nao";
  botao.append(
    elemento("span", "server-dispositivo-nome", nome),
    elemento("span", "server-dispositivo-marca"),
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
  for (const botao of lista.querySelectorAll(".server-dispositivo")) {
    const id = botao.dataset.dispositivo === "" ? null : botao.dataset.dispositivo;
    const escolhido = escolhidoDe[chave] === id;
    botao.dataset.escolhido = escolhido ? "sim" : "nao";

    let marca = "";
    if (aberto !== null && aberto === id) marca = "EM USO";
    else if (escolhido) marca = "ESCOLHIDO";
    else if (botao.dataset.padrao === "sim") marca = "PADRÃO";

    const alvo = botao.querySelector(".server-dispositivo-marca");
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
  const medidor = $("server-nivel");
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
 * Sem sessão os três ficam apagados: `set_voice_mode` fala com uma sessão
 * aberta, e não há preferência em disco que os lembre. Apagado **e** dizendo por quê, ou
 * a lacuna se lê como defeito.
 */
function desenharModos(snapshot) {
  const semSessao = !snapshot;
  for (const botao of document.querySelectorAll(".server-modo")) {
    botao.disabled = semSessao;
    botao.setAttribute("aria-pressed", !semSessao && snapshot.voice_mode === botao.dataset.modo);
    if (semSessao) {
      botao.title = "Só dá para escolher isto com uma conversa aberta.";
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
 * nome que se digita na entrada só vira o seu depois que o PERSISTENCE o vincula.
 */
function desenharIdentidade(snapshot) {
  const alvo = $("server-apelido");
  const apelido = snapshot?.nickname ?? "";
  alvo.textContent = apelido === "" ? "——" : apelido;
  alvo.classList.toggle("ausente", apelido === "");
}

// --------------------------------------------------------------------- ações

/** Escolhe um microfone, ou volta para o padrão da máquina com `null`. */
async function escolher(lado, id) {
  const erro = $("server-erro");
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
  await atualizarServer();
}

/** Troca o modo do microfone. O nome do modo volta como chegou. */
async function escolherModo(modo) {
  try {
    await invoke("set_voice_mode", { mode: modo });
  } catch (falha) {
    console.warn("set_voice_mode:", falha);
  }
  await atualizarServer();
}

/** Abre a configuração, lembrando de onde. */
async function abrirServer(origem) {
  // Com a tela de origem ainda visível, ou não há foco a lembrar: é o que
  // devolve a engrenagem — ou o TERMINAL SERVER da entrada — a quem a apertou.
  guardarFoco(origem);
  telaDeOrigem = origem;
  $(origem).hidden = true;
  $("tela-server").hidden = false;
  $("server-erro").hidden = true;
  // A recusa da seção do servidor some junto: ela é sobre o arquivo que alguém
  // escolheu da última vez, e reabrir a tela não é tentar de novo.
  $("server-servidor-erro").hidden = true;
  $("server-voltar-texto").textContent = VOLTA[origem] ?? "VOLTAR";
  await desenharDispositivos();
  await atualizarServer();
  abrirTela("tela-server");
}

/** Fecha e devolve para a tela que a abriu. */
function fecharServer() {
  guardarFoco("tela-server");
  $("tela-server").hidden = true;
  const volta = telaDeOrigem ?? "tela-boot";
  $(volta).hidden = false;
  telaDeOrigem = null;
  // A mesma porta, dos dois lados: quem entrou pela engrenagem da sessão sai
  // nela, e quem entrou pelo TERMINAL SERVER da entrada sai nele.
  voltarParaTela(volta);
  // Só quando se volta para a sessão: `desenharMensagens` sai cedo enquanto a
  // lista está sem layout, e redesenhar aqui é o outro lado desse acordo. Vindo
  // da tela de entrada não há sessão nenhuma a redesenhar.
  if (volta === "tela-sessao") {
    atualizar().catch((falha) => console.warn("voltar do Terminal server:", falha));
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
function abandonarServer() {
  $("tela-server").hidden = true;
  telaDeOrigem = null;
}

/**
 * Puxa o snapshot para o medidor, para quem está capturando, para os modos do
 * microfone e para o apelido.
 *
 * Sem sessão isto falha, e falhar é o estado normal desta tela quando aberta da
 * entrada — não é aviso de nada.
 */
async function atualizarServer() {
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
  desenharServidor(snapshot);
  await sincronizarIcone(snapshot);
}

// ------------------------------------------------------------- o servidor
//
// A única seção desta tela que não é desta máquina: o nome e a imagem do
// servidor, para quem tem permissão de mudá-los.
//
// ---- o que esta metade decide, e o que ela não decide ----
//
// Ela não decide **nada** sobre quem pode. `may_customise_server` chega pronto
// no snapshot, resolvido pelo PERMISSIONS a partir das permissões desta conexão, e
// esconder a seção não impede ninguém de nada: um pedido sem a permissão é
// recusado no servidor e volta como aviso. Isto é não oferecer o que não ia
// funcionar — a mesma decisão que a coluna de canais toma com os formulários de
// criar.
//
// Ela também não decide o que é uma imagem aceitável. Os dois números que a
// frase escreve vêm do Rust, e quem recusa uma imagem é o protocolo, com o
// número dele dentro do erro.

/**
 * O teto e o lado que o servidor aceita, como o Rust os conta.
 *
 * Buscados uma vez e guardados: são constantes de protocolo, e não mudam
 * enquanto este binário for este binário. `null` enquanto a resposta não
 * chegou — e nesse intervalo a frase fica vazia em vez de trazer números
 * inventados.
 */
let regrasDoIcone = null;

/**
 * A imagem que está desenhada, e a revisão que a produziu.
 *
 * `revisao` é o `icon_revision` do snapshot, e é o acordo inteiro: os bytes não
 * viajam no snapshot — ele atravessa a ponte em JSON duas vezes por segundo —,
 * então o que se compara é um número, e só quando ele anda é que
 * `icone_do_server` é chamado. `null` quer dizer «não há sessão que eu tenha
 * desenhado», e não «revisão zero»: um servidor novo começa a contar do zero de
 * novo, e sem essa distinção a imagem do servidor anterior ficaria no
 * cabeçalho do seguinte.
 */
const iconeDesenhado = { revisao: null, uri: null };

/**
 * Os bytes de um PNG como um `data:` que um `<img>` aceita.
 *
 * Montado aqui, tipo de mídia incluído, e isso seria errado para um anexo — lá
 * o tipo é uma **alegação** de quem mandou, e o ADR 0027 proíbe a tela de
 * juntar as duas coisas. Aqui não há alegação nenhuma para o conteúdo
 * desmentir: a mensagem do protocolo carrega bytes e mais nada, e o que ela
 * aceita é PNG e só PNG, conferido nas duas pontas pela assinatura. `image/png`
 * não é uma promessa desta linha; é o que estes bytes já provaram ser antes de
 * chegarem aqui.
 *
 * `data:` e não um blob porque a Content Security Policy desta janela é
 * `img-src 'self' data:` e não se mexe nela por causa de uma imagem.
 */
function uriDeIcone(bytes) {
  // Em pedaços, e não `String.fromCharCode(...bytes)`: espalhar oito mil
  // argumentos numa chamada estoura a pilha em alguns motores, e a falha
  // apareceria como a imagem simplesmente não desenhando.
  let cru = "";
  for (let de = 0; de < bytes.length; de += 4096) {
    cru += String.fromCharCode.apply(null, bytes.slice(de, de + 4096));
  }
  return `data:image/png;base64,${btoa(cru)}`;
}

/** Põe a imagem guardada nos dois lugares que a desenham, ou tira os dois. */
function pintarIcone() {
  const uri = iconeDesenhado.uri;
  for (const id of ["topo-server-icone", "server-icone-previa"]) {
    const alvo = $(id);
    if (uri) alvo.src = uri;
    else alvo.removeAttribute("src");
    alvo.hidden = !uri;
  }
  // O dizer só existe na tela de configuração: no cabeçalho, a ausência de
  // imagem é o cabeçalho de sempre, e não uma lacuna a explicar.
  $("server-icone-vazio").hidden = Boolean(uri);
}

/** Esquece a imagem desenhada. Toda sessão nova começa por aqui. */
function esquecerIcone() {
  if (iconeDesenhado.revisao === null && iconeDesenhado.uri === null) return;
  iconeDesenhado.revisao = null;
  iconeDesenhado.uri = null;
  pintarIcone();
}

/**
 * Busca os bytes **só** quando a revisão andou.
 *
 * É o precedente de `messages_revision`, e o custo que ele evita é concreto:
 * são até oito mil bytes atravessando a ponte em JSON, e um redesenho os
 * puxaria duas vezes por segundo para uma imagem que muda quando alguém aperta
 * um botão.
 */
async function sincronizarIcone(snapshot) {
  if (!snapshot) {
    esquecerIcone();
    return;
  }
  if (iconeDesenhado.revisao === snapshot.icon_revision) return;
  iconeDesenhado.revisao = snapshot.icon_revision;
  try {
    const bytes = await invoke("icone_do_server");
    iconeDesenhado.uri = bytes ? uriDeIcone(bytes) : null;
  } catch (falha) {
    // A sessão acabou entre o snapshot e esta chamada. Não é aviso de nada, e
    // a revisão volta a `null` para que a próxima leitura busque de novo.
    if (falha !== "NotConnected") console.warn("icone_do_server:", falha);
    iconeDesenhado.revisao = null;
    iconeDesenhado.uri = null;
  }
  pintarIcone();
}

/**
 * Puxa um snapshot só para a imagem, fora do laço desta tela.
 *
 * Chamado pelo `ServerChanged` e por mais nada. Uma volta de IPC por troca de
 * nome ou de imagem é barata; o que seria caro — e o que o `icon_revision`
 * existe para evitar — é buscar os **bytes**, e isso continua acontecendo só
 * quando o número anda.
 */
async function seguirOServidor() {
  let snapshot = null;
  try {
    snapshot = await invoke("snapshot");
  } catch (falha) {
    if (falha !== "NotConnected") console.warn("snapshot:", falha);
  }
  await sincronizarIcone(snapshot);
}

/**
 * Quem pode ver a seção, e o que ela mostra.
 *
 * O campo do nome traz o nome que **está valendo**, e não o que foi digitado —
 * é a mesma regra que faz a lista de microfones dizer qual abriu. Ele só não é
 * reescrito enquanto está sob o cursor: uma tela que sobrescreve o que alguém
 * está digitando, duas vezes por segundo, é uma tela em que ninguém consegue
 * escrever nada.
 */
function desenharServidor(snapshot) {
  const pode = snapshot?.may_customise_server === true;
  $("secao-servidor-item").hidden = !pode;

  // Perder a permissão — ou a sessão — com a seção aberta deixaria o painel de
  // pé com o botão que o abre já apagado da coluna. Volta para a primeira, que
  // é a que existe sempre.
  if (!pode && $("secao-servidor").getAttribute("aria-current") === "true") {
    abrirSecao("secao-audio");
  }
  if (!pode) return;

  const campo = $("server-nome-valor");
  if (document.activeElement !== campo) campo.value = snapshot.server ?? "";
}

/**
 * A frase da regra.
 *
 * **Sem os dois números**, agora que o app encolhe a imagem antes de enviá-la.
 * Eles continuam sendo a regra do protocolo e continuam vindo do Rust — o que
 * mudou é que deixaram de ser uma tarefa de quem escolhe. Dizer «no máximo
 * 8 KiB» a quem tem uma foto de 3 MB é enunciar um requisito sem dizer como
 * cumpri-lo, e a resposta honesta era «não sei, use outro programa».
 */
function desenharRegraDoIcone() {
  if (!regrasDoIcone) return;
  $("server-icone-regra").textContent =
    "Qualquer imagem. Ela é reduzida a um distintivo de até " +
    `${regrasDoIcone.lado} pixels e ${emKibibytes(regrasDoIcone.limite_bytes)} ` +
    "antes de ir para quem está conectado.";
}

/** `8 KiB` — o teto como uma pessoa o lê. */
function emKibibytes(bytes) {
  return `${Math.round(bytes / 1024)} KiB`;
}

/**
 * A frase de uma imagem recusada.
 *
 * As duas recusas são separadas porque o passo seguinte é outro: uma fotografia
 * dá para encolher, e um PDF não vira imagem por mais que se insista. E o
 * número sai do próprio erro, e não da cópia que o Rust guarda para escrever a
 * frase de cima: quem manda é o protocolo, e é o número dele que a pessoa tem
 * de acreditar.
 */
function fraseDeIcone(falha) {
  if (falha === "IconNotAPicture") {
    return (
      "ESTE ARQUIVO NÃO SERVE COMO IMAGEM DESTE SERVIDOR.\n" +
      "PNG, JPEG, WebP e GIF servem; o que o app não conseguir abrir como imagem, não."
    );
  }
  if (falha && typeof falha === "object" && falha.IconTooBig) {
    const teto = emKibibytes(falha.IconTooBig.limit_bytes);
    return `ESTA IMAGEM É PESADA DEMAIS.\nO máximo é ${teto}.`;
  }
  return fraseDeErro(falha);
}

/** Mostra o que deu errado, revelando antes de escrever. */
function erroDoServidor(falha) {
  const onde = $("server-servidor-erro");
  onde.hidden = false;
  onde.textContent = fraseDeIcone(falha);
}

/** Manda o nome que está na caixa. Sai do campo, vale. */
async function renomearServidor() {
  const campo = $("server-nome-valor");
  const nome = campo.value.trim();
  $("server-servidor-erro").hidden = true;
  // Vazio é desistir da edição, e não pedir um servidor sem nome: a FFI engole
  // o nome em branco antes de mandá-lo, e a tela devolve o que está valendo
  // para que a caixa não fique mentindo.
  if (nome === "") {
    await atualizarServer();
    return;
  }
  try {
    await invoke("renomear_server", { name: nome });
  } catch (falha) {
    erroDoServidor(falha);
  }
  await atualizarServer();
}

/** Abre o seletor do sistema e põe o que a pessoa escolher. */
async function escolherIcone() {
  const botao = $("server-icone-escolher");
  $("server-servidor-erro").hidden = true;
  botao.disabled = true;
  try {
    // `false` é ter fechado o seletor sem escolher, que é o desfecho mais
    // comum de todos e não é falha nenhuma.
    if (await invoke("escolher_icone_do_server")) {
      anunciar("Imagem do servidor trocada.");
    }
  } catch (falha) {
    erroDoServidor(falha);
  } finally {
    botao.disabled = false;
  }
  await atualizarServer();
}

/** Tira a imagem, deixando o servidor sem nenhuma. */
async function tirarIcone() {
  $("server-servidor-erro").hidden = true;
  try {
    await invoke("tirar_icone_do_server");
    anunciar("Este servidor ficou sem imagem.");
  } catch (falha) {
    erroDoServidor(falha);
  }
  await atualizarServer();
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
      "não dá para saber quanto falta — só quanto já veio",
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

$("botao-server").addEventListener("click", () => abrirServer("tela-boot"));
$("botao-server-sessao").addEventListener("click", () => abrirServer("tela-sessao"));
$("server-fechar").addEventListener("click", fecharServer);

for (const botao of document.querySelectorAll(".server-secao")) {
  botao.addEventListener("click", () => abrirSecao(botao.id));
}

for (const botao of document.querySelectorAll(".server-modo")) {
  botao.addEventListener("click", () => escolherModo(botao.dataset.modo));
}

$("atualizacao-procurar").addEventListener("click", procurarAtualizacao);
$("atualizacao-instalar").addEventListener("click", instalarAtualizacao);

$("server-icone-escolher").addEventListener("click", escolherIcone);
$("server-icone-tirar").addEventListener("click", tirarIcone);

// Sair do campo é mandar. `change` é o evento que diz as duas coisas de uma vez
// — o valor mudou **e** a edição terminou —, e é ele que faz o Enter e o clique
// fora valerem a mesma coisa sem um botão no meio. Um valor escrito pelo script
// não o dispara, então o redesenho de meio em meio segundo não se manda de volta
// para o servidor.
$("server-nome-valor").addEventListener("change", renomearServidor);
// O Enter só tira o foco; quem manda continua sendo o `change` acima. Sem isto
// o cursor fica na caixa depois de renomear, e nada na tela diz que acabou.
$("server-nome-valor").addEventListener("keydown", (evento) => {
  if (evento.key === "Enter") $("server-nome-valor").blur();
});

// Os dois números da imagem, uma vez. São constantes do protocolo — não mudam
// enquanto este binário for este binário —, e a frase que os usa fica vazia até
// eles chegarem, em vez de trazer números escritos à mão que possam divergir.
invoke("regras_do_icone_do_server")
  .then((regras) => {
    regrasDoIcone = regras;
    desenharRegraDoIcone();
  })
  .catch((falha) => console.warn("regras_do_icone_do_server:", falha));

// A imagem do cabeçalho tem de acompanhar a sessão, e o cabeçalho fica na tela
// **ao lado** desta: o laço lá embaixo só roda com a configuração na frente. Daí
// este ouvinte, e daí ele ser estreito — `ServerChanged` é o único evento que a
// imagem pode ter mudado, e ele não chega por telemetria nem por mensagem.
//
// `ConnectStageChanged` é o zerar, e é ele porque é o único aviso que existe
// **antes** de haver sessão. Sem isto, entrar num servidor sem imagem logo
// depois de sair de um que tinha deixaria a imagem do anterior no cabeçalho do
// seguinte: a revisão de um servidor novo recomeça do zero, e nada chegaria para
// contradizer o que já estava desenhado.
listen("seele://event", (evento) => {
  const payload = evento.payload;
  if (payload === "ServerChanged") {
    seguirOServidor().catch((falha) => console.warn("ServerChanged:", falha));
    return;
  }
  if (payload && typeof payload === "object" && (payload.Ended || payload.ConnectStageChanged)) {
    esquecerIcone();
  }
});

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
  if (evento.key === "Escape" && !$("tela-server").hidden) {
    evento.preventDefault();
    fecharServer();
  }
});

// O estado inicial da tela, escrito uma vez com ela ainda escondida: a seção de
// áudio aberta. Nenhum quadro chega a mostrar o cabeçalho vazio.
abrirSecao("secao-audio");

// O nível de entrada muda sozinho, e é a única coisa viva nesta tela. Mesmo
// meio segundo da telemetria da sessão, e só com a tela na frente: um `invoke`
// duas vezes por segundo para uma tela escondida é uma volta de IPC por nada.
setInterval(() => {
  if (!$("tela-server").hidden) atualizarServer();
}, 500);
