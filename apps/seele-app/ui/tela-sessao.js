// SEELE · Entry Plug — a tela de operação (`#tela-sessao`).
//
// O projetor: cada `desenhar*` recebe o snapshot inteiro e escreve a tela, sem
// estado derivado. Também a busca, a faixa de veredito, a bateria interna, o
// push-to-talk e a caixa de convite de quem hospeda — tudo que só existe
// enquanto há sessão.
//
// ---- o que esta tela desenha, e o que ela recusa a desenhar ----
//
// A composição é a do comp v2 (`design/Entry Plug v2.dc.html`, tela
// `principal`), inventariada em `.superpowers/sdd/comp-inventario.md` §3.
// Quatro colunas: a ficha do Dogma, os canais, a Linha aberta, a Taxa de
// Sincronização.
//
// Boa parte do que o comp desenha não tem dado por trás — o inventário §16
// conta 23 valores nessa situação. A regra seguida aqui é uma só: **desenhar a
// moldura e deixar o valor visivelmente não medido**. Nada de número plausível,
// nada de campo apagado da tela. `crates/seele-core/src/state.rs` guarda essa
// mesma ideia num teste cujo nome é a frase inteira — uma taxa não medida lê
// zero, e não cem. Um travessão com `title` é o oposto de um número inventado:
// ele diz que ninguém mediu, e diz por quê.
//
// O que ficou sem dado, e o que cada um exigiria, está em
// `.superpowers/sdd/tela-dogma.md`.

"use strict";

/** O último snapshot desenhado, para não redesenhar o que não mudou. */
let desenhado = null;
/** A Linha aberta, para saber para onde vai o que se digita. */
let linhaAberta = null;
/** Se a barra de espaço já está segurando o microfone. */
let falando = false;
/**
 * Quando esta sessão começou a ser desenhada, para o campo `UPTIME`.
 *
 * O inventário §16 classifica o uptime como **local**: não é um valor que
 * atravesse a fronteira, é o relógio desta máquina contando desde a conexão.
 * O instante registrado é o do primeiro snapshot desenhado, que é a primeira
 * volta de IPC depois de `connect` — a diferença para o instante da conexão é o
 * tempo de um `await`, e não vale um campo novo no protocolo para corrigir.
 *
 * `null` fora de sessão, e zerado ao ejetar: um uptime que sobrevive à sessão
 * conta o tempo de outro Dogma.
 *
 * `comeco`, e a escolha da palavra não é gosto: `tests/frontend.rs` proíbe a
 * tradução portuguesa de `start` em qualquer script, porque é assim que o campo
 * de um `Match` chegaria se alguém traduzisse os nomes que a busca serializa —
 * e ali a tradução desenha `<mark>` vazio, calada, sem erro em lugar nenhum. O
 * guarda é literal de propósito, e a palavra que ele veta não tem sinônimo
 * seguro: este relógio não tem nada a ver com busca e mesmo assim o reprovava.
 */
let comecoDaSessao = null;
/** Volume por apelido, para o deslizante não pular de volta a cada redesenho. */
const volumes = new Map();
/**
 * Os casamentos da busca corrente, agrupados por índice de mensagem.
 *
 * Vem prontos do Rust. Os deslocamentos são em **caracteres** e valem sobre o
 * corpo normalizado — ver `corpoComRealce`.
 */
let casamentosPorMensagem = new Map();
/**
 * Qual mensagem e qual ocorrência dentro dela o cursor da busca está.
 *
 * `{ indice, ordinal }` — o índice da mensagem na ordem em que a tela desenha
 * e qual ocorrência dentro dela. `null` sem busca. Os dois números vêm prontos do
 * Rust — o ordinal é o `Search::ordinal_in_message` do core. Aqui não se conta
 * nada: contar seria a mesma contagem que decide o `[n/m]`, escrita de novo e
 * livre para discordar.
 */
let ocorrenciaAtual = null;

// ------------------------------------------------- o valor que ninguém mediu

/** O que se escreve onde havia para escrever um número e não há. */
const SEM_MEDIDA = "—";

/**
 * Marca um campo como não medido, e diz o que faltaria para medi-lo.
 *
 * O travessão é o mesmo que `quando()` já usava em `base.js`, e é deliberado
 * que ele **não** se pareça com um valor: um `0`, um `--` ou um `···` no lugar
 * de uma contagem de operadores seriam lidos como "zero operadores", "ainda
 * carregando" e "medindo", e nenhuma das três é verdade. A verdade é que este
 * produto não tem por onde saber.
 *
 * O motivo vai no `title` e não num rodapé: ele responde a uma pergunta que só
 * quem reparou no travessão está fazendo.
 */
function naoMedido(nodo, motivo) {
  nodo.textContent = SEM_MEDIDA;
  nodo.classList.add("sem-dado");
  nodo.title = motivo;
}

/** O contrário: um valor que existe, no mesmo campo. */
function medido(nodo, texto) {
  nodo.textContent = texto;
  nodo.classList.remove("sem-dado");
  nodo.removeAttribute("title");
}

/**
 * A barra de blocos do comp: `n` células, cheias na proporção de `pct`.
 *
 * Igual à `blocos()` do comp e à da TUI, e a igualdade é o ponto — 20 blocos
 * são 5% cada nos dois clientes, e quem aprendeu a ler a barra no terminal lê
 * a mesma barra aqui. Nunca sozinha: o número que ela desenha está sempre ao
 * lado (`specs/05-cliente-tui.md`).
 */
function blocos(pct, total) {
  const cheios = Math.max(0, Math.min(total, Math.round((pct / 100) * total)));
  return "█".repeat(cheios) + "░".repeat(total - cheios);
}

// ------------------------------------------------------------------ veredito

/**
 * A frase de um veredito de identidade, ou `null` quando não há o que dizer.
 *
 * A comparação já aconteceu — em Rust, dentro do `connect`, antes de isto ser
 * chamado. O que chega aqui é o resultado dela mais a impressão digital para
 * uma pessoa conferir por outro canal, que é a única coisa que se faz com uma
 * impressão digital (ADR 0003). Nada nesta função compara nada.
 *
 * `Known` não vira frase de propósito: repetir "a chave é a mesma de sempre" a
 * cada entrada é ensinar a não ler a linha no dia em que ela não for.
 *
 * A recusa de um convite que aponta para outra chave não chega como veredito —
 * ela derruba a conexão, e a frase dela vive em `fraseDeErro`.
 */
function fraseDoVeredito(veredito) {
  if (!veredito || veredito === "Known") return null;

  // `tofu.rs`: a casca tem que dizer o que acabou de confiar. Fixar em
  // silêncio é fixar sem ninguém saber que havia o que conferir.
  if (veredito.FirstContact) {
    return (
      "PRIMEIRO CONTATO — CHAVE FIXADA\n" +
      `${veredito.FirstContact.fingerprint}\n` +
      "Ninguém confirmou que é esta. Confira com quem hospeda, por outro canal."
    );
  }
  // É para produzir exatamente isto que o link do ADR 0006 existe. Não há nada
  // a fazer, e por isso a frase não pede nada.
  if (veredito.FirstContactVerified) {
    return (
      "PRIMEIRO CONTATO VERIFICADO — O CONVITE CONFIRMOU A CHAVE\n" +
      veredito.FirstContactVerified.fingerprint
    );
  }
  // O Dogma é o mesmo de sempre; o link é que não é dele. Ressalva, não queda.
  if (veredito.InviteDisagrees) {
    return (
      "O CONVITE NÃO CORRESPONDE A ESTE DOGMA.\n" +
      `esperada: ${veredito.InviteDisagrees.expected}\n` +
      `ofertada: ${veredito.InviteDisagrees.offered}\n` +
      "Você entrou no Dogma de sempre. O link é que não leva a ele."
    );
  }
  return null;
}

/** Acende — ou apaga — a faixa de veredito da sessão. */
function mostrarVeredito(veredito) {
  const frase = fraseDoVeredito(veredito);
  // Mostrar a faixa **antes** de escrever nela. Um `role="status"` só é lido
  // quando o texto muda com a região já visível: escrever primeiro e revelar
  // depois faz vários leitores de tela tratarem a mudança como conteúdo que
  // sempre esteve ali, e o veredito passa calado por quem mais precisa dele.
  $("veredito").hidden = frase === null;
  $("veredito-texto").textContent = frase ?? "";
}

// ------------------------------------------------------------------- desenho

function desenhar(snapshot) {
  if (!snapshot) return;

  if (snapshot.ended) {
    mostrarFim(snapshot.ended);
    return;
  }

  // O primeiro quadro da sessão é onde o uptime começa a contar.
  if (comecoDaSessao === null) comecoDaSessao = Date.now();

  desenharTopo(snapshot);
  desenharFicha(snapshot);
  desenharCanais(snapshot);
  desenharOperador(snapshot);
  desenharLinha(snapshot);
  desenharMensagens(snapshot);
  desenharSync(snapshot);
  desenharTelemetria(snapshot);
  desenharAviso(snapshot);

  desenhado = snapshot;
}

function desenharTopo(snapshot) {
  const padrao = $("padrao");
  padrao.dataset.padrao = snapshot.pattern;
  padrao.textContent = {
    Offline: "PADRÃO: DESLIGADO",
    Orange: "PADRÃO: LARANJA",
    Blue: "PADRÃO: AZUL",
  }[snapshot.pattern];

  $("topo-piloto").textContent = snapshot.nickname;
}

/**
 * A ficha da sala — `C·02 / DOGMA`.
 *
 * Cinco campos, três medidos. `OPERADORES` e `ROTA` continuam desenhados e
 * vazios: ver o bloco no topo deste arquivo.
 */
function desenharFicha(snapshot) {
  $("dogma-nome").textContent = snapshot.dogma || SEM_MEDIDA;

  // O `Snapshot` só conhece quem está sentado em Cage. Não há população do
  // Dogma em lugar nenhum do protocolo, e somar os assentos ocupados daria uma
  // contagem menor com cara de contagem certa.
  naoMedido(
    $("resumo-operadores"),
    "o protocolo não carrega a população do Dogma, só quem está sentado em Cage",
  );

  // Estes dois o snapshot tem inteiros: `cages` e `lines` chegam completos.
  medido($("resumo-cages"), String(snapshot.cages.length).padStart(2, "0"));
  medido($("resumo-linhas"), String(snapshot.lines.length).padStart(2, "0"));

  const codec = $("resumo-codec");
  if (!snapshot.audio_available) {
    // Ausência conhecida, e não medida por falta: esta sessão não tem áudio, e
    // dizer "SEM ÁUDIO" é um fato, não um buraco.
    medido(codec, "SEM ÁUDIO");
  } else {
    medido(codec, `OPUS ${Math.round(snapshot.telemetry.bitrate_bps / 1000)}k`);
  }

  // O comp mostra `BALTHASAR·01` em sete lugares. O conceito de rota não existe
  // no core — nem por Dogma, nem por piloto.
  naoMedido($("resumo-rota"), "o protocolo não tem o conceito de rota");
}

/**
 * Os Cages e as Linhas, em duas listas com cabeçalho próprio.
 *
 * A ocupação de **todo** Cage é desenhável hoje, e não só a do ocupado:
 * `cages_of` popula `pilots` a partir de `room.roster(cage.id)` para cada Cage
 * (inventário §16). Era o que o app escondia num `title=`.
 */
function desenharCanais(snapshot) {
  const cages = snapshot.cages.map((cage) => {
    const item = elemento("li", cage.occupied_by_us ? "cage aberto" : "cage");
    item.dataset.cage = String(cage.id);

    const cabeca = elemento("span", "canal-cabeca");
    cabeca.append(elemento("span", "cage-nome", cage.name));

    // A ocupação é o acompanhante textual da barra: a barra sozinha é forma, e
    // `4/8` é o número que sobrevive a qualquer paleta.
    const ocupacao = cage.limit > 0 ? (cage.pilots.length / cage.limit) * 100 : 0;
    cabeca.append(elemento("span", "cage-ocupacao", `${cage.pilots.length}/${cage.limit}`));

    const barra = elemento("span", "barra", blocos(ocupacao, 12));
    barra.setAttribute("aria-hidden", "true");

    item.append(cabeca, barra);
    return item;
  });
  repovoar($("lista-cages"), cages);

  const linhas = snapshot.lines.map((linha) => {
    const item = elemento("li", linha.open ? "linha aberto" : "linha");
    item.dataset.linha = String(linha.id);
    item.append(elemento("span", "linha-rotulo", linha.name));

    // As pendências por Linha são a marca laranja do comp. `Line` é
    // `{id, name, open}` — não há contagem de não-lidas nem marca d'água de
    // leitura em lugar nenhum do core.
    const pendencias = elemento("span", null);
    naoMedido(pendencias, "o protocolo não carrega pendências por Linha");
    item.append(pendencias);

    if (linha.open) linhaAberta = linha.id;
    return item;
  });
  repovoar($("lista-linhas"), linhas);
}

/** A tira do operador, no rodapé da coluna de canais. */
function desenharOperador(snapshot) {
  $("operador-nome").textContent = snapshot.nickname;

  // Os rótulos são os do comp: o botão diz em que estado o A.T. Field está, e
  // não o que apertá-lo vai fazer. Um botão escrito com o verbo é um botão que
  // ninguém sabe ler quando volta a olhar para a tela.
  const mudo = $("botao-mudo");
  mudo.textContent = snapshot.at_field ? "A.T. FIELD ATIVO" : "A.T. FIELD INATIVO";
  mudo.dataset.ativo = snapshot.at_field ? "sim" : "nao";

  const surdo = $("botao-surdo");
  surdo.textContent = snapshot.total_isolation ? "ISOLAMENTO TOTAL" : "OUVINDO";
  surdo.dataset.ativo = snapshot.total_isolation ? "sim" : "nao";

  const voz = $("botao-voz");
  // "MODO:" na frente porque `TECLA` sozinho não diz que é um seletor — e um
  // seletor que ninguém reconhece como seletor é um botão que ninguém aperta.
  voz.textContent = { PushToTalk: "MODO: TECLA", VoiceActivated: "MODO: VOZ", Open: "MODO: ABERTO" }[
    snapshot.voice_mode
  ] ?? "TECLA";
  voz.disabled = !snapshot.audio_available;
  voz.dataset.ativo = snapshot.voice_mode === "Open" ? "sim" : "nao";

  const falar = $("botao-falar");
  falar.dataset.ativo = snapshot.speaking ? "sim" : "nao";
  // O rótulo é a instrução. Um botão escrito "FALAR" que não faz nada ao ser
  // clicado é pior que nenhum botão: ensina a coisa errada.
  $("falar-rotulo").textContent = snapshot.speaking
    ? "NO AR"
    : { PushToTalk: "SEGURE ESPAÇO", VoiceActivated: "FALE", Open: "MICROFONE ABERTO" }[
        snapshot.voice_mode
      ] ?? "SEGURE ESPAÇO";
  falar.disabled = !snapshot.audio_available;
  falar.title = snapshot.audio_available
    ? "segure a barra de espaço, ou este botão"
    : "esta sessão não tem áudio";
}

/** A barra de 40px da Linha aberta. */
function desenharLinha(snapshot) {
  const aberta = snapshot.lines.find((linha) => linha.open);
  $("linha-nome").textContent = `LINHA ${aberta ? aberta.name : SEM_MEDIDA}`;
}

function desenharMensagens(snapshot) {
  const lista = $("lista-mensagens");
  // Só rola sozinho se já estava no fim: puxar alguém de volta para baixo no
  // meio de uma leitura é pior do que não acompanhar.
  const noFim = lista.scrollHeight - lista.scrollTop - lista.clientHeight < 32;

  const itens = snapshot.messages.map((mensagem, indice) => {
    // A grade do comp: uma coluna de 76px para a hora, o resto para autor e
    // corpo. A marca de 2px à esquerda é onde o comp distingue mensagem de
    // sistema e de alerta — `Message` não tem tipo (inventário §16), então só
    // duas larguras existem aqui: a própria e a dos outros.
    const item = elemento("li", mensagem.own ? "mensagem propria" : "mensagem");
    item.append(elemento("span", "mensagem-hora", relogio(mensagem.at_seconds)));

    const conteudo = elemento("span", "mensagem-conteudo");
    const cabeca = elemento("span", "mensagem-autor", mensagem.author_nickname);
    conteudo.append(cabeca);
    if (mensagem.edited) conteudo.append(elemento("span", "editada", "editada"));

    // O corpo **cru**, e isto não é detalhe de pintura. `.mensagens .corpo` é
    // `white-space: pre-wrap`: esta janela mostra quebra de linha e espaço
    // duplo como eles chegaram, e é essa string que a busca do outro lado da
    // ponte recebeu. Colapsar aqui deslocaria o realce em toda mensagem de mais
    // de uma linha; colapsar dos dois lados alinharia o realce achatando a
    // conversa, que é o preço errado a pagar por um índice.
    const corpo = elemento("span", "corpo");
    const aceso = ocorrenciaAtual?.indice === indice ? ocorrenciaAtual.ordinal : null;
    corpo.append(...corpoComRealce(mensagem.body, casamentosPorMensagem.get(indice), aceso));
    conteudo.append(corpo);

    item.append(conteudo);
    return item;
  });

  repovoar(lista, itens);
  if (noFim) lista.scrollTop = lista.scrollHeight;
}

/**
 * Parte o corpo em pedaços aceso e apagado.
 *
 * Recebe os intervalos prontos do Rust: com dobramento de acento e de caixa o
 * frontend não teria como saber onde o casamento começou. Os deslocamentos são
 * em caracteres, não em unidades de código — daí o `[...corpo]`, que é o que
 * mantém o realce no lugar num corpo com emoji.
 *
 * `aceso` é qual destes intervalos é o do cursor, ou `null` se o cursor está
 * noutra mensagem. Sem ele todas as ocorrências saíam idênticas e o piloto não
 * enxergava onde estava dentro de uma mensagem que casa três vezes. A ordem
 * desta lista é a mesma em que o core contou, e é o que faz o índice bater.
 *
 * **Casamentos sobrepostos reemitem caractere.** Para "aa" em "aaa",
 * `occurrences` devolve `(0,2)` e `(1,3)`. Nesta função, ao contrário de
 * `ui.rs` no terminal, não há guarda para `start < cursor`: o segundo
 * casamento fatia `caracteres.slice(1, 3)` sem descontar que o índice 1 já
 * saiu no primeiro `<mark>`, e o caractere sobreposto é desenhado duas vezes —
 * "aaa" digitado pela pessoa vira "aaaa" na tela. `docs/pendencias.md` #14
 * registra isto; não é corrigido aqui.
 */
function corpoComRealce(corpo, intervalos, aceso = null) {
  if (!intervalos || intervalos.length === 0) return [document.createTextNode(corpo)];
  const caracteres = [...corpo];
  const pedacos = [];
  let cursor = 0;
  for (const [ordinal, { start, end }] of intervalos.entries()) {
    if (start > cursor) {
      pedacos.push(document.createTextNode(caracteres.slice(cursor, start).join("")));
    }
    const classe = ordinal === aceso ? "realce realce-atual" : "realce";
    pedacos.push(elemento("mark", classe, caracteres.slice(start, end).join("")));
    cursor = end;
  }
  if (cursor < caracteres.length) {
    pedacos.push(document.createTextNode(caracteres.slice(cursor).join("")));
  }
  return pedacos;
}

// ------------------------------------------------- taxa de sincronização

/**
 * O painel da direita: a média do Cage e uma linha por piloto.
 *
 * A média **não é calculada aqui**. Ela chega em `cage.sync` já com faixa e com
 * o tamanho da amostra, decidida uma vez no core — `types.rs` argumenta que
 * duas cascas com duas cópias de "85 é nominal" são duas cascas que discordam
 * no dia em que uma delas for atualizada, e o comp faz exatamente essa cópia
 * (`corSync(media)` na casca). `null` quando o Cage está vazio: um Cage sem
 * ninguém não tem média, e zero pintaria toda sala parada de vermelho.
 */
function desenharSync(snapshot) {
  const cage = snapshot.cages.find((c) => c.occupied_by_us);

  desenharMedia(cage);

  // Dentro de um Cage, o roster é quem está nele. Fora, é o próprio operador:
  // a Taxa de Sincronização é a medida que `specs/05-cliente-tui.md` chama de
  // permanente, e sumir com ela porque ninguém inseriu o plug ainda seria
  // escondê-la justo enquanto se decide em qual Cage entrar.
  const pilotos = cage
    ? cage.pilots.map((piloto) => ({
        nome: piloto.nickname + (piloto.is_self ? " (você)" : ""),
        ratio: piloto.sync_ratio,
        faixa: piloto.sync_band,
        falando: piloto.speaking,
        atField: piloto.at_field,
        isolado: piloto.total_isolation,
        // O deslizante é dos outros: baixar o próprio volume não faz nada,
        // porque a própria voz nunca entra na mistura (`specs/03-audio.md`).
        volume: piloto.is_self ? null : piloto.nickname,
      }))
    : [
        {
          nome: `${snapshot.nickname} (você)`,
          ratio: snapshot.telemetry.sync_ratio,
          faixa: snapshot.telemetry.sync_band,
          falando: snapshot.speaking,
          atField: snapshot.at_field,
          isolado: snapshot.total_isolation,
          volume: null,
        },
      ];

  repovoar(
    $("lista-roster"),
    pilotos.map((piloto) => linhaDoRoster(piloto, snapshot.audio_available)),
  );
}

/** O bloco invertido de 52px, e a legenda ao lado dele. */
function desenharMedia(cage) {
  const bloco = $("sync-media-bloco");
  const valor = $("sync-media-valor");
  const marca = $("sync-media-marca");
  const amostra = $("sync-amostra");

  if (!cage || !cage.sync) {
    // Sem plug inserido, ou num Cage vazio. Não é uma média baixa: é a ausência
    // de qualquer coisa para tirar média de.
    delete bloco.dataset.faixa;
    marca.textContent = "";
    naoMedido(valor, cage ? "este Cage está vazio" : "nenhum plug inserido");
    naoMedido(amostra, cage ? "este Cage está vazio" : "nenhum plug inserido");
    return;
  }

  const sync = cage.sync;
  bloco.dataset.faixa = sync.band;
  // A marca de bloco é a metade que sobrevive sem cor. Ela fica em face
  // monoespaçada ao lado do número porque a Saira Condensed, que desenha o
  // número, não tem `U+2588`.
  marca.textContent = marcaSync(sync.band);
  medido(valor, String(sync.ratio));
  medido(amostra, `${sync.pilots} ${sync.pilots === 1 ? "PLUG" : "PLUGS"}`);
}

/**
 * Uma linha do roster.
 *
 * Quatro faixas de informação, e todas as quatro têm acompanhante textual: o
 * número ao lado da marca de bloco, a barra de 20 blocos, o atraso, e a
 * pastilha de estado. Nenhuma delas depende de enxergar a cor
 * (`specs/05-cliente-tui.md`).
 */
function linhaDoRoster(piloto, temAudio) {
  const item = elemento("li", piloto.falando ? "piloto falando" : "piloto");
  item.dataset.faixa = piloto.faixa;

  const cabeca = elemento("span", "piloto-cabeca");
  const identidade = elemento("span", "piloto-identidade");
  identidade.append(elemento("span", "piloto-nome", piloto.nome));

  // `MELCHIOR·01` no comp. `Pilot` não tem o subsistema que atende o piloto, e
  // o protocolo não carrega qual seria.
  const tag = elemento("span", "piloto-tag");
  naoMedido(tag, "o protocolo não diz qual subsistema atende cada piloto");
  identidade.append(tag);

  const numero = elemento("span", "piloto-sync");
  // A marca de bloco antes do número, pela mesma razão que na média: a Saira
  // desenha o número e não tem o bloco.
  numero.append(
    elemento("span", "sync-marca", marcaSync(piloto.faixa)),
    // Inteiro, e não `98.4`: `sync_ratio` é `u8` em todo ponto onde existe, e
    // uma casa decimal aqui seria precisão inventada no último passo.
    elemento("span", "piloto-sync-valor", String(piloto.ratio)),
  );

  cabeca.append(identidade, numero);

  const barra = elemento("span", "barra", blocos(piloto.ratio, 20));
  barra.setAttribute("aria-hidden", "true");

  const rodape = elemento("span", "piloto-rodape");
  const atraso = elemento("span", "piloto-atraso");
  atraso.append(elemento("b", "rotulo-micro", "ATRASO"), document.createTextNode(" "));
  const valorAtraso = elemento("span", null);
  // `Telemetry` é a **nossa** conexão. Latência por par não atravessa a
  // fronteira e não é derivável de nada que atravesse.
  naoMedido(valorAtraso, "o RTT medido é o desta máquina, não o de cada piloto");
  atraso.append(valorAtraso);

  const estados = elemento("span", "piloto-estados");
  // A pastilha do comp: bloco sólido com texto no negro absoluto, e não texto
  // colorido. `PLUG EJETADO` é o quarto estado do comp e não aparece aqui —
  // quem sai some de `cage.pilots`, e manter a lápide exigiria ou um campo de
  // estado no `Pilot`, ou esta casca lembrando de quem estava ali, que é
  // exatamente o estado derivado que o topo de `base.js` proíbe.
  const estado = piloto.atField
    ? "A.T. FIELD ATIVO"
    : piloto.falando
      ? "TRANSMITINDO"
      : "EM ESCUTA";
  const pastilha = elemento("span", "pastilha", estado);
  pastilha.dataset.estado = piloto.atField ? "at" : piloto.falando ? "fala" : "escuta";
  estados.append(pastilha);

  // O isolamento total não existe no comp e existe no produto. Segunda
  // pastilha, e não uma troca da primeira: estar surdo e estar transmitindo são
  // dois fatos ao mesmo tempo, e um deles apagando o outro esconderia metade.
  if (piloto.isolado) {
    const surdez = elemento("span", "pastilha", "ISOLAMENTO TOTAL");
    surdez.dataset.estado = "surdo";
    estados.append(surdez);
  }

  rodape.append(atraso, estados);
  item.append(cabeca, barra, rodape);

  // Volume por pessoa (`specs/03-audio.md`).
  if (piloto.volume !== null && temAudio) {
    const volume = document.createElement("input");
    volume.type = "range";
    volume.className = "volume";
    volume.min = "0";
    volume.max = "200";
    volume.step = "10";
    volume.value = String(volumes.get(piloto.volume) ?? 100);
    volume.title = `volume de ${piloto.volume}`;
    volume.dataset.piloto = piloto.volume;
    item.append(volume);
  }

  return item;
}

// ---------------------------------------------------------------- telemetria

function desenharTelemetria(snapshot) {
  const tel = snapshot.telemetry;

  medido($("tel-rtt"), `${Math.round(tel.rtt_ms)}ms`);
  medido($("tel-jit"), `${Math.round(tel.jitter_ms)}ms`);
  medido($("tel-loss"), `${(tel.loss_fraction * 100).toFixed(1)}%`);

  // `snapshot.audio_available`, e não `tel.audio_available`: o campo é do
  // `Snapshot` e nunca existiu no `Telemetry`. A comparação antiga lia
  // `undefined === false`, que é sempre falso — a sessão sem áudio imprimia
  // `0k` como se fosse uma medida.
  medido(
    $("tel-codec"),
    snapshot.audio_available ? `OPUS ${Math.round(tel.bitrate_bps / 1000)}k` : "SEM ÁUDIO",
  );

  medido($("tel-uptime"), duracao(comecoDaSessao));

  const cage = snapshot.cages.find((c) => c.occupied_by_us);
  medido($("tel-cage"), cage ? cage.name : "SEM PLUG");

  $("tel-local").hidden = !tel.local_fault;

  desenharEnlace(snapshot.link);
}

/** `HH:MM:SS` desde um instante local, ou o travessão antes de haver um. */
function duracao(desde) {
  if (desde === null) return SEM_MEDIDA;
  const total = Math.max(0, Math.floor((Date.now() - desde) / 1000));
  const horas = String(Math.floor(total / 3600)).padStart(2, "0");
  const minutos = String(Math.floor((total % 3600) / 60)).padStart(2, "0");
  const segundos = String(total % 60).padStart(2, "0");
  return `${horas}:${minutos}:${segundos}`;
}

function desenharAviso(snapshot) {
  const banner = $("banner");
  if (!snapshot.notice) {
    banner.hidden = true;
    return;
  }
  const aviso = snapshot.notice;
  banner.hidden = false;
  banner.dataset.severidade = aviso.severity;
  $("banner-texto").textContent = aviso.operator_text ?? AVISOS[aviso.reason] ?? "AVISO";
}

/**
 * A bateria interna, desenhada sobre a sessão.
 *
 * `specs/07-tema-evangelion.md` proíbe fechar ou trocar de tela quando a
 * conexão cai: esmaece, conta, e deixa o histórico legível. Por isso isto só
 * acende uma faixa e uma classe no corpo — nada some.
 */
function desenharEnlace(link) {
  const faixa = $("bateria");
  if (!link || link === "Online") {
    faixa.hidden = true;
    document.body.classList.remove("na-bateria");
    return;
  }

  const bateria = link.InternalBattery;
  if (!bateria) return;

  const restam = Math.max(0, bateria.remaining_seconds);
  const minutos = String(Math.floor(restam / 60)).padStart(2, "0");
  const segundos = String(restam % 60).padStart(2, "0");
  // Visível primeiro, escrita depois — a mesma ordem da faixa de veredito, e
  // pela mesma razão: um `role="status"` escondido no instante em que o texto
  // muda não anuncia nada.
  faixa.hidden = false;
  $("bateria-conta").textContent = `${minutos}:${segundos}`;
  // As tentativas listadas, que a spec pede por nome. Zero ainda é informação:
  // quer dizer que a primeira está em curso.
  $("bateria-tentativas").textContent =
    bateria.attempts === 0 ? "reconectando…" : `${bateria.attempts} tentativas`;
  document.body.classList.add("na-bateria");
}

// --------------------------------------------------------------------- ações

async function enviar(evento) {
  evento.preventDefault();
  const campo = $("campo-mensagem");
  const corpo = campo.value.trim();
  if (!corpo || linhaAberta === null) return;

  // Limpa antes de esperar a resposta: um campo que só esvazia depois do ida e
  // volta parece travado numa rede ruim, que é justo quando não pode parecer.
  campo.value = "";
  try {
    await invoke("send_message", { line: linhaAberta, body: corpo });
  } catch (falha) {
    campo.value = corpo;
    console.warn("send_message:", falha);
  }
}

async function alternarCanal(evento) {
  const item = evento.target.closest("li");
  if (!item) return;
  try {
    if (item.dataset.cage) {
      // Clicar no Cage em que já se está é sair dele — a mesma tecla entra e
      // sai, como no `plug`.
      const cage = Number(item.dataset.cage);
      if (item.classList.contains("aberto")) {
        await invoke("eject_plug");
      } else {
        await invoke("insert_plug", { cage });
      }
    } else if (item.dataset.linha) {
      linhaAberta = Number(item.dataset.linha);
      await invoke("open_line", { line: linhaAberta });
    }
    // Soltos **antes** do redesenho, não depois: ver `soltarCasamentos`.
    soltarCasamentos();
    await atualizar();
    // A lista de mensagens acabou de ser trocada inteira. Ver `refazerBusca`.
    await refazerBusca();
  } catch (falha) {
    console.warn("canal:", falha);
  }
}

/**
 * Push-to-talk.
 *
 * A janela relata teclas soltas de verdade, então aqui é segurar de fato — sem
 * a trava que o ADR 0016 precisou inventar para terminais que não relatam.
 */
function segurarFala(segurando) {
  if (falando === segurando) return;
  falando = segurando;
  $("botao-falar").dataset.ativo = segurando ? "sim" : "nao";
  invoke("set_talking", { talking: segurando }).catch(() => {});
}

/** Ejeta e volta para a tela de entrada, sem fechar o programa. */
async function ejetar() {
  await invoke("disconnect");
  $("tela-sessao").hidden = true;
  $("tela-fim").hidden = true;
  $("tela-boot").hidden = false;
  $("convite").hidden = true;
  $("bateria").hidden = true;
  // O veredito era sobre a chave daquela sessão. Deixá-lo aceso sobre a
  // próxima seria dizer de um Dogma o que se apurou de outro.
  mostrarVeredito(null);
  document.body.classList.remove("na-bateria");
  desenhado = null;
  linhaAberta = null;
  // O mesmo argumento do veredito, para o relógio: um uptime que sobrevive à
  // sessão conta o tempo passado noutro Dogma.
  comecoDaSessao = null;
  await encerrarBusca();
  // O convite não sobrevive à sessão que ele abriu: quem sai, digita outro
  // endereço e aperta INSERT mandaria o token do Dogma anterior ao novo.
  limparConvite();
  // Quem acabou de sair de um Dogma tem que vê-lo na lista.
  await desenharVisitados();
}

/** Zera o campo, o cursor no Rust e o realce. */
async function encerrarBusca() {
  $("campo-busca").value = "";
  await invoke("busca_limpar");
  limparBusca();
}

// --------------------------------------------------------------------- busca

/**
 * Reagrupa os casamentos por mensagem, que é como o desenho os alcança.
 *
 * O cursor — qual das ocorrências está selecionada, e a regra de dar a volta nas
 * pontas — vive no Rust. Aqui não há decisão nenhuma sobre busca.
 */
function guardarCasamentos(estado) {
  casamentosPorMensagem = new Map();
  for (const casamento of estado.casamentos) {
    const lista = casamentosPorMensagem.get(casamento.message) ?? [];
    lista.push(casamento);
    casamentosPorMensagem.set(casamento.message, lista);
  }
  ocorrenciaAtual =
    estado.atual && estado.ordinal !== null && estado.ordinal !== undefined
      ? { indice: estado.atual.message, ordinal: estado.ordinal }
      : null;
}

function desenharBusca(estado) {
  // `[0/0]` e não vazio: "não achei" é informação, e um contador que some
  // parece uma busca que não rodou.
  $("busca-contador").textContent = `[${estado.posicao}/${estado.total}]`;
  guardarCasamentos(estado);
  if (desenhado) desenharMensagens(desenhado);
  if (estado.atual) {
    // A ocorrência, e não a mensagem. Rolar até a mensagem punha na tela a
    // linha certa e nada dentro dela: numa mensagem que casa três vezes,
    // avançar duas vezes rolava para o mesmo lugar e mexia só no algarismo.
    const alvo =
      $("lista-mensagens").querySelector(".realce-atual") ??
      $("lista-mensagens").children[estado.atual.message];
    alvo?.scrollIntoView({ block: "center" });
  }
}

function limparBusca() {
  casamentosPorMensagem = new Map();
  ocorrenciaAtual = null;
  $("busca-contador").textContent = "";
  if (desenhado) desenharMensagens(desenhado);
}

/** Há uma busca de pé? */
function buscaAtiva() {
  return $("campo-busca").value.trim() !== "";
}

/**
 * Solta os casamentos antes de um redesenho que troca a lista.
 *
 * `atualizar()` repinta com o que estiver no mapa, e só uma volta de IPC depois
 * é que `refazerBusca` conserta — mas o quadro do meio chega a aparecer,
 * acendendo trechos das mensagens erradas. Soltar antes troca um realce errado
 * visível por nenhum realce durante um quadro.
 */
function soltarCasamentos() {
  casamentosPorMensagem = new Map();
  ocorrenciaAtual = null;
}

/**
 * Refaz a busca sobre o histórico que está na tela agora.
 *
 * Obrigatório sempre que a lista desenhada mudar de forma — abrir outra Linha,
 * entrar ou sair de um Cage, uma mensagem editada ou apagada. Ao contrário do
 * `plug`, que recalcula o realce a partir do termo a cada desenho
 * (`seele-tui::ui`) e só guarda o cursor, esta janela guarda os deslocamentos
 * que vieram do Rust; sem um ponto de invalidação eles passariam a acender
 * trechos de mensagens que não são mais aquelas, com `[n/m]` contando uma
 * conversa que saiu da tela.
 *
 * O termo continua. O que se recalcula é todo o resto.
 */
async function refazerBusca() {
  const termo = $("campo-busca").value;
  if (termo.trim() === "") {
    await invoke("busca_limpar");
    limparBusca();
    return;
  }
  try {
    desenharBusca(await invoke("buscar", { termo }));
  } catch (falha) {
    // Sem sessão não há histórico para buscar. Não é erro de ninguém.
    if (falha !== "NotConnected") console.warn("buscar:", falha);
  }
}

// ------------------------------------------------------------------- ligação

$("form-busca").addEventListener("submit", (evento) => evento.preventDefault());

$("campo-busca").addEventListener("input", refazerBusca);

$("busca-proxima").addEventListener("click", async () =>
  desenharBusca(await invoke("busca_andar", { adiante: true })),
);
$("busca-anterior").addEventListener("click", async () =>
  desenharBusca(await invoke("busca_andar", { adiante: false })),
);
$("form-mensagem").addEventListener("submit", enviar);
// Duas listas, um manipulador: os Cages e as Linhas ganharam cabeçalhos
// próprios (`B·03` e `B·04`) e deixaram de caber numa lista só.
$("lista-cages").addEventListener("click", alternarCanal);
$("lista-linhas").addEventListener("click", alternarCanal);
$("banner-fechar").addEventListener("click", () => ($("banner").hidden = true));
$("veredito-fechar").addEventListener("click", () => ($("veredito").hidden = true));

$("botao-mudo").addEventListener("click", async () => {
  const snapshot = await invoke("snapshot");
  await invoke("set_at_field", { on: !snapshot.at_field });
  await atualizar();
});

$("botao-surdo").addEventListener("click", async () => {
  const snapshot = await invoke("snapshot");
  await invoke("set_total_isolation", { on: !snapshot.total_isolation });
  await atualizar();
});

// O deslizante manda enquanto arrasta: ouvir o efeito é o que diz onde parar.
// Ele mora no roster agora, que é onde o piloto está.
$("lista-roster").addEventListener("input", (evento) => {
  const alvo = evento.target;
  if (!alvo.classList.contains("volume")) return;
  const percent = Number(alvo.value);
  volumes.set(alvo.dataset.piloto, percent);
  invoke("set_volume", { nickname: alvo.dataset.piloto, percent }).catch((falha) => {
    console.warn("set_volume:", falha);
  });
});

// TECLA → VOZ → ABERTO → TECLA. `specs/03-audio.md` faz de push-to-talk o
// padrão porque ele nunca dispara sozinho; sair dele é sempre um ato explícito.
$("botao-voz").addEventListener("click", async () => {
  const snapshot = await invoke("snapshot");
  const proximo = { PushToTalk: "VoiceActivated", VoiceActivated: "Open", Open: "PushToTalk" }[
    snapshot.voice_mode
  ] ?? "PushToTalk";
  await invoke("set_voice_mode", { mode: proximo });
  await atualizar();
});

$("botao-falar").addEventListener("pointerdown", () => segurarFala(true));
$("botao-falar").addEventListener("pointerup", () => segurarFala(false));
$("botao-falar").addEventListener("pointerleave", () => segurarFala(false));

$("convite-copiar").addEventListener("click", async () => {
  const campo = $("convite-link");
  // `select()` antes de tudo: se a área de transferência for negada, a pessoa
  // ainda fica com o link selecionado e copia com o teclado.
  campo.select();
  const botao = $("convite-copiar");
  try {
    await navigator.clipboard.writeText(campo.value);
    botao.textContent = "copiado";
    botao.classList.add("convite-copiado");
  } catch {
    // A tecla desenhada, e com nome: ela não está ao lado de um rótulo, ela é
    // metade da instrução. Sem nome a frase chegaria a um leitor de tela como
    // "copie com C", que manda apertar a tecla errada.
    botao.replaceChildren(
      document.createTextNode("copie com "),
      glifo("comando", "Command"),
      document.createTextNode("C"),
    );
  }
});

$("botao-trocar").addEventListener("click", ejetar);

// A barra de espaço fala, exceto enquanto se digita — a mesma colisão que a TUI
// resolve mantendo o push-to-talk fora do modo de inserção (decisão D19).
window.addEventListener("keydown", (evento) => {
  // `/` foca a busca, como no terminal. Só fora de um campo de texto — uma
  // barra digitada numa mensagem é uma barra — e só com a sessão na tela, ou
  // engoliria a tecla para focar um campo que está escondido.
  if (evento.key === "/" && !digitando() && !$("tela-sessao").hidden) {
    evento.preventDefault();
    $("campo-busca").focus();
    return;
  }
  if (evento.target === $("campo-busca")) {
    if (evento.key === "Escape") {
      evento.preventDefault();
      encerrarBusca();
      $("campo-busca").blur();
      return;
    }
    if (evento.key === "Enter") {
      // Enter anda; Shift+Enter volta — o `n`/`N` do `plug`, no teclado que
      // esta janela tem.
      evento.preventDefault();
      invoke("busca_andar", { adiante: !evento.shiftKey }).then(desenharBusca);
      return;
    }
  }
  if (evento.code === "Space" && !digitando() && !evento.repeat) {
    evento.preventDefault();
    segurarFala(true);
  }
});
window.addEventListener("keyup", (evento) => {
  if (evento.code === "Space" && !digitando()) segurarFala(false);
});
// Uma janela que perde o foco com o microfone aberto é um microfone esquecido.
window.addEventListener("blur", () => segurarFala(false));

// O core diz o que mudou; isto só redesenha. Um snapshot por evento é barato e
// evita que a tela e o estado discordem.
listen("seele://event", (evento) => {
  const payload = evento.payload;
  if (payload && typeof payload === "object" && payload.Ended) {
    mostrarFim(payload.Ended.reason);
    return;
  }

  // Uma mensagem editada troca um corpo no lugar e uma apagada encurta a lista:
  // nos dois casos os deslocamentos guardados passam a apontar para outro texto,
  // que é a mesma falha que trocar de Linha causava, por outra porta. Mensagem
  // acrescentada não desloca nada — entra no fim —, mas o evento é um só e não
  // diz qual das três foi.
  //
  // Só com uma busca de pé, e só neste evento: refazer a busca a cada tique de
  // telemetria seria uma volta de IPC por nada, duas vezes por segundo.
  if (payload === "MessagesChanged" && buscaAtiva()) {
    soltarCasamentos();
    atualizar()
      .then(refazerBusca)
      .catch((falha) => console.warn("busca:", falha));
    return;
  }

  atualizar();
});

// O relógio do topo é do relógio local, não do servidor.
setInterval(() => {
  $("relogio").textContent = new Date().toLocaleTimeString();
}, 1000);

// A telemetria muda sozinha entre eventos — nível de entrada, RTT, deriva. O
// uptime anda junto: ele é redesenhado por `desenharTelemetria`, e não por um
// segundo temporizador que contaria em paralelo e sairia de fase.
setInterval(() => {
  if (!$("tela-sessao").hidden) atualizar();
}, 500);
