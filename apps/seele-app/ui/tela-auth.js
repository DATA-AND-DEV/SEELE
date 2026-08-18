// SEELE · Entry Plug — a entrada em Dogma Central (`#tela-auth`).
//
// Duas entradas e dois trabalhos, escolhidos por `data-modo` na `<section>`.
//
// `aperto` — chega pelo `connect` bem-sucedido de `tela-boot.js` e sai para
// `#tela-sessao`. É o aperto de mão virando etapa: antes dela, o único vestígio
// dele na janela era o `PADRÃO: AZUL` no cabeçalho da sessão, escrito depois de
// tudo já ter acontecido, inclusive de o plug já estar dentro de um Cage.
//
// `espera` — chega pelo `connect` **derrubado** com `AdmissionPending`, a
// portaria do ADR 0030. É trabalho novo, e nasceu de um teste de verdade: um
// amigo bateu, leu a frase certa numa linha de erro da tela de entrada, e ficou
// sem saber o que fazer com ela. Aqui a mesma frase tem tela, e a tela responde
// as três perguntas de quem espera — o que aconteceu, o que fazer agora, e o
// que não adianta fazer.
//
// **Nada tenta de novo sozinho.** Uma tela que repete a tentativa transforma o
// «nada espera» do ADR 0030 em espera com outro nome, e ainda bate na porta do
// Dogma de outra pessoa sem parar, contra os baldes de fichas do ADR 0025. Cada
// batida é um botão apertado por alguém.
//
// ---- o que esta tela sabe, e o que ela deixou de desenhar ----
//
// O `connect` devolve duas coisas: o snapshot e o veredito da chave. As duas são
// fato apurado, e são o que os painéis mostram.
//
// Quatro campos do comp v2 saíram, e a saída é a correção de uma convenção
// aplicada longe demais. Eles eram desenhados com a moldura de pé e o valor
// escrito como ausente — a contagem de operadores do Dogma, a rota, o codec e a
// chave de identidade local —, e nenhum deles existe: os dois primeiros não são
// conceito em lugar nenhum do protocolo, o codec só ganha valor depois de o
// plug entrar num Cage e nesta tela ninguém entrou ainda, e a chave local não
// atravessa a FFI. Mostrar a falta serve a uma lacuna que se pretende fechar;
// nenhuma destas se fechava, e sete campos com quatro travessões não ensinam
// nada — dão uma tela que parece quebrada. A lacuna fica registrada aqui e no
// `index.html`, que é onde lacuna se registra.
//
// O carimbo de hora das linhas do B·02 é o relógio local no instante em que
// **esta janela** observou o fato. Não é um tempo que veio do Dogma, e não
// pretende ser: é estado da casca, da mesma natureza do relógio do cabeçalho.
// Em `espera` é ele que guarda o histórico das tentativas, que é a única coisa
// que essa tela tem para contar e ninguém mais conta.

"use strict";

/** O que o `connect` devolveu, entre a chegada nesta tela e a saída dela. */
let aperto = null;

/**
 * As três frases do padrão da sessão.
 *
 * `Snapshot.pattern` já é exatamente `Offline` / `Orange` / `Blue`, decidido no
 * core — a casca não classifica nada aqui, só escolhe como dizer.
 *
 * As frases de `Orange` e `Blue` são as do comp, palavra por palavra. `Offline`
 * não tem frase no comp: lá a tela só conhece dois padrões, porque lá nunca há
 * uma sessão que ainda não abriu. Aqui há.
 */
const PADROES = {
  Offline: {
    rotulo: "PADRÃO: DESLIGADO",
    kanji: "",
    nota: "Sem sessão aberta. Nada a verificar até o Dogma responder.",
  },
  Orange: {
    rotulo: "PADRÃO: LARANJA",
    kanji: "橙",
    nota: "Sessão não verificada. Captura suspensa até MELCHIOR confirmar a chave.",
  },
  Blue: {
    rotulo: "PADRÃO: AZUL",
    kanji: "青",
    nota: "Identidade confirmada pelos três subsistemas. Captura liberada.",
  },
};

/** O travessão duplo: a moldura está desenhada e o valor não existe. */
const AUSENTE = "——";

/**
 * Entra na tela de autenticação com o que o `connect` acabou de devolver.
 *
 * Chamada de `tela-boot.js`, de dentro do manipulador do formulário — que é
 * sempre depois de os sete arquivos terem rodado, e é o que permite a uma tela
 * chamar a outra sem `import` (ADR 0019).
 */
function entrarNaAutenticacao(snapshot, veredito, endereco) {
  aperto = { snapshot, veredito };

  guardarFoco("tela-boot");
  $("tela-boot").hidden = true;
  $("tela-auth").hidden = false;
  // De volta ao aperto de mão, inclusive quando se chega aqui pelo botão de
  // tentar de novo da espera: a admissão saiu, e a tela que a esperava não tem
  // mais assunto.
  $("tela-auth").dataset.modo = "aperto";
  $("auth-parede").hidden = true;

  $("auth-endereco").textContent = endereco || AUSENTE;
  // A sessão precisa do mesmo endereço para a porta do cabeçalho, e este é o
  // último ponto do caminho que ainda o tem: o `Snapshot` não o carrega.
  guardarAlvoDoDogma(endereco);
  desenharPadrao(snapshot);
  desenharDogmaDaEntrada(snapshot);

  // O primeiro passo do botão: ler o que o aperto de mão decidiu. O segundo,
  // que é o que insere o plug, só existe depois dele.
  const botao = $("auth-botao");
  botao.dataset.passo = "verificar";
  botao.textContent = "VERIFICAR IDENTIDADE";
  botao.disabled = false;

  // Uma linha, e ela é verdade: este endereço foi resolvido, agora, por esta
  // janela. As outras quatro do `AUTH_LOG_BASE` do comp descrevem etapas de um
  // aperto de mão que o core faz inteiro do lado de lá, sem relatar nenhuma.
  repovoar($("auth-registro"), []);
  registrar(`RESOLVENDO ${endereco || AUSENTE}`, "apagado");

  // No fim, e não junto do `hidden`: o alvo é o `auth-botao`, e ele acabou de
  // ser reescrito e reabilitado duas dúzias de linhas acima.
  abrirTela("tela-auth");
}

/** A cartela de padrão, a nota que a acompanha, e o botão que a veste. */
function desenharPadrao(snapshot) {
  const padrao = PADROES[snapshot?.pattern] ?? PADROES.Offline;
  const cartela = $("auth-padrao");
  cartela.dataset.padrao = snapshot?.pattern ?? "Offline";
  $("auth-padrao-rotulo").textContent = padrao.rotulo;
  $("auth-padrao-kanji").textContent = padrao.kanji;
  $("auth-padrao-nota").textContent = padrao.nota;
  $("auth-botao").dataset.padrao = snapshot?.pattern ?? "Offline";
}

/**
 * A ficha do Dogma — C·02.
 *
 * Três valores, e os três saem do snapshot: o nome do Dogma, quantos Cages e
 * quantas Linhas. O travessão que sobra é o do carregamento, e não o de um
 * valor que não existe: um snapshot sem lista é um snapshot que ainda não
 * chegou.
 *
 * O codec estava aqui e saiu. Ele lia `telemetry.bitrate_bps`, que só ganha
 * valor depois de o plug entrar num Cage — e nesta tela ninguém entrou ainda,
 * então ele saía `——` em toda entrada que já aconteceu. Um campo que é sempre
 * travessão é um campo, e não uma lacuna: a telemetria de verdade está na barra
 * permanente da sessão, que é onde ela tem o que medir.
 */
function desenharDogmaDaEntrada(snapshot) {
  $("auth-dogma-nome").textContent = snapshot?.dogma || AUSENTE;
  $("auth-cages").textContent = doisDigitos(snapshot?.cages?.length);
  $("auth-linhas").textContent = doisDigitos(snapshot?.lines?.length);
}

/** `03`, como o comp escreve — e `——` quando não há lista para contar. */
function doisDigitos(quantos) {
  return typeof quantos === "number" ? String(quantos).padStart(2, "0") : AUSENTE;
}

/**
 * Uma linha no B·02, com o relógio local do instante em que ela foi observada.
 *
 * `tom` escolhe a cor, e nunca é o que carrega a informação: o texto da linha
 * diz o que aconteceu por escrito (`specs/05-cliente-tui.md`).
 */
function registrar(texto, tom) {
  const linha = elemento("li", "registro-linha");
  const hora = new Date().toLocaleTimeString([], {
    hour: "2-digit",
    minute: "2-digit",
    second: "2-digit",
  });
  const corpo = elemento("span", "registro-texto", texto);
  if (tom) corpo.dataset.tom = tom;
  linha.append(elemento("span", "registro-hora", hora), corpo);
  $("auth-registro").append(linha);
}

/**
 * O primeiro passo: dizer o que o aperto de mão decidiu.
 *
 * Nenhuma comparação acontece aqui — ela já aconteceu, em Rust, dentro do
 * `connect` (ADR 0003), e o que chegou foi o resultado mais a impressão digital
 * para uma pessoa conferir por outro canal. O passo existe porque esse
 * resultado merece uma tela antes de o plug entrar, e não uma tarja depois.
 */
async function verificarIdentidade() {
  const frase = fraseDoVeredito(aperto?.veredito);
  if (frase) {
    registrar(frase, "atencao");
  } else {
    // `Known` não vira frase de propósito (`tela-sessao.js`): repetir "a chave é
    // a mesma de sempre" a cada entrada é ensinar a não ler a linha no dia em
    // que ela não for. O que se diz é que a conferência aconteceu.
    registrar("CHAVE CONFERIDA — NADA MUDOU DESDE A ÚLTIMA ENTRADA");
  }

  // O padrão pode ter mudado entre o `connect` e agora; o valor volta a ser
  // lido em vez de deduzido. `atualizar()` não serve porque desenha a sessão.
  try {
    const snapshot = await invoke("snapshot");
    aperto.snapshot = snapshot;
    desenharPadrao(snapshot);
    desenharDogmaDaEntrada(snapshot);
    registrar(PADROES[snapshot.pattern]?.rotulo ?? PADROES.Offline.rotulo, tomDoPadrao(snapshot));
  } catch (falha) {
    console.warn("snapshot:", falha);
  }

  const botao = $("auth-botao");
  botao.dataset.passo = "inserir";
  // «ENTRAR NO DOGMA», e não «INSERIR PLUG», porque inserir o plug deixou de
  // acontecer aqui: entrar numa jaula virou ato à parte, apertado por quem
  // quer voz. Um botão que promete uma coisa e faz outra é pior que um botão
  // sem nome — e este prometia ligar o microfone.
  botao.textContent = "ENTRAR NO DOGMA";
}

function tomDoPadrao(snapshot) {
  if (snapshot.pattern === "Blue") return "azul";
  if (snapshot.pattern === "Orange") return "atencao";
  return "apagado";
}

/**
 * O segundo passo: a sessão começa.
 *
 * **A Linha abre sozinha; a jaula não.** As duas coisas já foram automáticas
 * juntas, com um motivo bom — chegar numa tela vazia é chegar sem saber o que
 * fazer. O motivo continua bom para uma delas e nunca foi bom para a outra.
 *
 * Texto é passivo: ninguém te ouve por você estar lendo, então abrir a primeira
 * Linha resolve a tela vazia sem te comprometer com nada.
 *
 * Entrar numa jaula não é passivo. É ocupar uma das quinze vagas, aparecer na
 * lista como presente, e pôr um microfone à disposição de uma conversa que você
 * não escolheu. Quem chegou num Dogma para ler o que perdeu acordava dentro de
 * uma sala de voz sem ter apertado nada — foi o relato de quem usou: «não dá
 * para você ficar fora de uma sala».
 *
 * As duas coisas eram uma só aqui e são duas no modelo mental de quem usa.
 * Agora são duas aqui também.
 */
async function inserirPlug() {
  const botao = $("auth-botao");
  botao.disabled = true;
  try {
    const snapshot = aperto?.snapshot;
    if (snapshot?.lines?.length > 0) {
      await invoke("open_line", { line: snapshot.lines[0].id });
    }

    $("tela-auth").hidden = true;
    $("tela-sessao").hidden = false;
    // O veredito continua indo para a faixa da sessão. Ele foi lido aqui por
    // quem estava olhando esta tela; a faixa é onde ele fica disponível para
    // quem chegou depois, e é `role="status"`, que esta lista não é.
    mostrarVeredito(aperto?.veredito ?? null);
    await atualizar();
    // Depois do desenho, e sem `data-foco`: a operação recebe o foco na própria
    // `<section>`. Ver a marcação dela — focar o compositor aqui desligaria o
    // push-to-talk no primeiro segundo de sessão.
    abrirTela("tela-sessao");
  } catch (falha) {
    registrar(fraseDeErro(falha), "atencao");
  } finally {
    botao.disabled = false;
  }
}

// ---------------------------------------------------- a espera pela portaria
//
// ADR 0030. Quem bate num Dogma com portaria é derrubado **no instante**, de
// propósito: segurar a conexão obrigaria a um prazo, e um prazo fabrica
// «ninguém atendeu», que é uma resposta sobre a qual não se pode agir. O que o
// desenho não tinha era o outro lado disso — a pessoa caía de volta na tela de
// entrada com uma linha de erro, e uma linha de erro não diz que voltar amanhã
// funciona.
//
// A conexão continua caindo. O que muda é que ela cai numa tela.

/** A primeira linha de uma frase de duas — o que houve, sem o que fazer. */
function tituloDaFrase(frase) {
  return frase.split("\n")[0];
}

/**
 * Leva quem está esperando aprovação para esta tela, ou atualiza a espera que
 * já está na frente. Devolve se tratou a falha.
 *
 * Chamada do `catch` do `connect`, e é ela que decide de quem é a falha. Duas
 * condições, e a segunda é a que não é óbvia:
 *
 * - `AdmissionPending` **sempre** vem para cá, venha de onde vier. É a resposta
 *   que tem o que dizer e não cabe numa linha.
 * - Qualquer falha vem para cá **se esta tela já estiver na frente em espera**.
 *   Quem apertou TENTAR ENTRAR DE NOVO e não alcançou o Dogma precisa ler isso
 *   aqui: `#boot-erro` está atrás desta tela, e uma mensagem escrita numa tela
 *   escondida é uma mensagem que ninguém recebe.
 *
 * `AdmissionDenied` chegando da tela de entrada **não** vem para cá, e a
 * assimetria é deliberada: uma recusa não é uma espera, não há o que
 * acompanhar, e a tela de entrada é onde se escolhe outro Dogma. Chegando aqui,
 * numa tentativa feita de dentro da espera, ela é o fim desta espera e é dita
 * aqui — mudar de tela para dar uma resposta que a pessoa acabou de pedir seria
 * arrancá-la do lugar em que ela perguntou.
 */
function levarParaAEspera(falha, endereco) {
  const tela = $("tela-auth");
  const naEspera = !tela.hidden && tela.dataset.modo === "espera";
  const razao = falha?.Refused?.reason;
  if (razao !== "AdmissionPending" && !naEspera) return false;

  const chegando = tela.hidden;
  const frase = fraseDeErro(falha);
  // Não há aperto de mão nenhum: o `connect` não devolveu snapshot nem veredito,
  // e deixar os de uma conexão anterior aqui seria mostrar a ficha de um Dogma
  // em que esta pessoa não entrou.
  aperto = null;
  tela.dataset.modo = "espera";
  $("auth-endereco").textContent = endereco || AUSENTE;
  $("auth-espera-frase").textContent = frase;

  // A decisão saiu e foi não. O botão continua desenhado — um botão que some é
  // um botão que a pessoa procura — e fica desabilitado com o motivo escrito
  // embaixo, que é como esta janela já recusa apagar o último Cage.
  const recusado = razao === "AdmissionDenied";
  const botao = $("auth-botao");
  botao.dataset.passo = "tentar";
  botao.textContent = "TENTAR ENTRAR DE NOVO";
  botao.disabled = recusado;
  $("auth-parede").hidden = !recusado;
  $("auth-parede").textContent = recusado
    ? "Apertar de novo bate na mesma porta e recebe a mesma resposta, e cada " +
      "batida conta contra você no Dogma. Quem pode voltar atrás é quem hospeda."
    : "";

  if (chegando) {
    guardarFoco("tela-boot");
    $("tela-boot").hidden = true;
    // Escrito por `$("tela-auth")` e não pela variável acima, para o guarda que
    // conta transições enxergar esta: ele procura um id de tela seguido de
    // `.hidden = false`, e uma transição que ele não lê é uma transição que
    // pode perder o foco do teclado sem que nada reporte.
    $("tela-auth").hidden = false;
    repovoar($("auth-registro"), []);
    registrar(`RESOLVENDO ${endereco || AUSENTE}`, "apagado");
  }
  registrar(tituloDaFrase(frase), "atencao");

  // `abrirTela` só na chegada. Numa tentativa feita daqui o teclado já está
  // nesta tela, e devolvê-lo ao botão arrancaria de quem tivesse tabulado até a
  // saída — a notícia, essa, é anunciada nas duas.
  if (chegando) abrirTela("tela-auth", frase);
  else anunciar(frase);
  return true;
}

/**
 * Bate de novo, uma vez.
 *
 * Reaproveita o `conectar` da tela de entrada inteiro — mesmo endereço, mesmo
 * apelido, mesmo convite —, e é ele quem decide para onde isto vai: admitido,
 * `entrarNaAutenticacao` reescreve esta tela como aperto de mão; ainda não,
 * `levarParaAEspera` a reescreve como espera. Por isso não há `finally`
 * reabilitando o botão aqui: as duas saídas já o deixaram como deve ficar, e
 * reabilitar por cima desfaria a única coisa que uma recusa tem para dizer.
 *
 * Uma tentativa por aperto, e nenhum relógio: ver o cabeçalho deste arquivo.
 */
async function baterDeNovo() {
  $("auth-botao").disabled = true;
  registrar("BATENDO DE NOVO", "apagado");
  await conectar();
}

/**
 * Sai da espera pela porta por onde se entrou.
 *
 * Existe só em `espera`, e existe porque ali não há sessão: quem espera pode
 * querer outro Dogma, outro apelido, ou só fechar o assunto. No aperto de mão
 * não há esta saída, e não deve haver — lá a sessão está aberta, e o caminho de
 * volta é encerrá-la.
 */
function voltarParaAEntrada() {
  guardarFoco("tela-auth");
  $("tela-auth").hidden = true;
  $("tela-boot").hidden = false;
  voltarParaTela("tela-boot");
}

// ------------------------------------------------------------------- ligação

$("auth-botao").addEventListener("click", async () => {
  const passo = $("auth-botao").dataset.passo;
  if (passo === "tentar") await baterDeNovo();
  else if (passo === "inserir") await inserirPlug();
  else await verificarIdentidade();
});

$("auth-voltar").addEventListener("click", voltarParaAEntrada);
