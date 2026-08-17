// SEELE · Entry Plug — a entrada em Dogma Central (`#tela-auth`).
//
// Chega aqui pelo `connect` bem-sucedido de `tela-boot.js`, e sai daqui para
// `#tela-sessao`. É o aperto de mão virando etapa: hoje o único vestígio dele na
// janela é o `PADRÃO: AZUL` no cabeçalho da sessão, escrito depois de tudo já
// ter acontecido — inclusive de o plug já estar dentro de um Cage.
//
// ---- o que esta tela sabe, e o que ela não inventa ----
//
// O `connect` devolve duas coisas: o snapshot e o veredito da chave. As duas
// são fato apurado, e são o que os três painéis mostram. Tudo o mais que o comp
// desenha aqui — a contagem de operadores do Dogma, a rota, a chave de
// identidade local, as oito linhas carimbadas do log de aperto de mão — não
// existe em lugar nenhum do protocolo, e nenhuma delas é preenchida com um
// valor plausível. A moldura fica; o valor falta, e diz que falta.
//
// O carimbo de hora das linhas do B·02 é o relógio local no instante em que
// **esta janela** observou o fato. Não é um tempo que veio do Dogma, e não
// pretende ser: é estado da casca, da mesma natureza do relógio do cabeçalho.

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
 * Quatro dos sete valores do comp saem do snapshot: o nome do Dogma, quantos
 * Cages, quantas Linhas e o codec. Os outros três não existem: a população
 * total do Dogma, a rota, e a taxa de amostragem que acompanha o bitrate. Os
 * dois primeiros estão escritos como ausentes no `index.html`; o terceiro é a
 * razão pela qual o codec sai `OPUS 96k` e não `OPUS 96k / 48kHz`.
 */
function desenharDogmaDaEntrada(snapshot) {
  $("auth-dogma-nome").textContent = snapshot?.dogma || AUSENTE;
  $("auth-cages").textContent = doisDigitos(snapshot?.cages?.length);
  $("auth-linhas").textContent = doisDigitos(snapshot?.lines?.length);

  // O bitrate só existe depois de o plug entrar num Cage — antes disso a
  // telemetria é a de uma conexão que ainda não carrega áudio nenhum. Zero aqui
  // não é "zero kbps", é "ainda não há o que medir".
  const bits = snapshot?.telemetry?.bitrate_bps ?? 0;
  const codec = $("auth-codec");
  codec.textContent = bits > 0 ? `OPUS ${Math.round(bits / 1000)}k` : AUSENTE;
  codec.classList.toggle("ausente", bits <= 0);
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
  botao.textContent = "INSERIR PLUG";
}

function tomDoPadrao(snapshot) {
  if (snapshot.pattern === "Blue") return "azul";
  if (snapshot.pattern === "Orange") return "atencao";
  return "apagado";
}

/**
 * O segundo passo: o plug entra, e a sessão começa.
 *
 * É o mesmo que a entrada fazia sozinha antes desta tela existir — entrar no
 * primeiro Cage e abrir a primeira Linha, porque chegar numa tela vazia é
 * chegar sem saber o que fazer.
 */
async function inserirPlug() {
  const botao = $("auth-botao");
  botao.disabled = true;
  try {
    const snapshot = aperto?.snapshot;
    if (snapshot?.cages?.length > 0) {
      await invoke("insert_plug", { cage: snapshot.cages[0].id });
    }
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

// ------------------------------------------------------------------- ligação

$("auth-botao").addEventListener("click", async () => {
  if ($("auth-botao").dataset.passo === "inserir") await inserirPlug();
  else await verificarIdentidade();
});
