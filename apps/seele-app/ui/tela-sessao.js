// SEELE · Entry Plug — a tela de operação (`#tela-sessao`).
//
// O projetor: cada `desenhar*` recebe o snapshot inteiro e escreve a tela, sem
// estado derivado. Também a busca, a faixa de veredito, a bateria interna, o
// push-to-talk e a caixa de convite de quem hospeda — tudo que só existe
// enquanto há sessão.
//
// ---- o que esta tela desenha, e o que ela recusa a desenhar ----
//
// A composição é a do comp **v3** (`design/Entry Plug v3.dc.html`, tela
// `principal`), inventariada em `.superpowers/sdd/comp-inventario-v3.md` §6.
// Quatro colunas — a trilha de servidores, as salas e os canais, o canal
// aberto, a faixa de pessoas — em `60px 268px minmax(0,1fr) 328px`.
//
// A quarta era o painel de sinal, e a troca é o achado da avaliação de
// usabilidade: numa ferramenta de comunicação, a faixa permanente estava com o
// dado de diagnóstico e a lista de pessoas não existia em tela nenhuma. Agora
// ela é das pessoas, o sinal de cada uma é uma linha dentro do cartão dela, e a
// média da sala desceu para o rodapé de telemetria.
//
// Em janela estreita elas não apertam todas juntas, e a ordem é decisão: a
// faixa de pessoas recolhe primeiro, a coluna de salas e canais vira gaveta
// depois, e a conversa é a última a ceder largura. Ver `alternarCanais` aqui e
// o rodapé de `tela-sessao.css`.
//
// O que o v3 muda de fundo, e é do que trata metade deste arquivo: cada sala de voz
// mostra **quem está dentro** antes de se entrar nele, entrar e sair viraram
// botões com rótulo, as mensagens ganharam avatar de iniciais, e a busca deixou
// de viver aberta.
//
// ---- a regra do valor que não existe, e a inversão do v3 ----
//
// O v2 mandava desenhar a moldura e deixar o valor visivelmente não medido: um
// travessão com `title`, nunca um número plausível.
// `crates/seele-core/src/state.rs` guarda a outra metade dessa ideia num teste
// cujo nome é a frase inteira — uma taxa não medida lê zero, e não cem.
//
// Ela continua valendo onde a ausência **responde** a uma pergunta que a tela
// acabou de fazer: a média sem plug inserido, a barra da bateria, as três
// células do alerta. E ela foi invertida onde a ausência se repetia por
// fileira — pendências por Linha, subsistema por pessoa, atraso por pessoa —,
// porque meia dúzia de travessões explicados numa tela que existe para ser
// simples é ruído, não honestidade. Ver o cabeçalho de `tela-sessao.css`.
//
// O que ficou sem dado, e o que cada um exigiria, está em
// `.superpowers/sdd/tela-server.md`.

"use strict";

/** O último snapshot desenhado, para não redesenhar o que não mudou. */
let desenhado = null;
/** A Linha aberta, para saber para onde vai o que se digita. */
let linhaAberta = null;
/**
 * O endereço que esta janela discou, para a porta do cabeçalho.
 *
 * Não vem do `Snapshot`: o protocolo não carrega para onde nos conectamos. Quem
 * o tem é a tela de autenticação, que o recebeu da tela de entrada, e ela o
 * entrega aqui no instante em que abre a sessão. `null` fora de sessão, e
 * apagado ao ejetar — um endereço que sobrevive à sessão diz do próximo Server
 * a porta do anterior.
 */
let alvoDoServer = null;

/** A tela de autenticação diz em que endereço esta sessão está começando. */
function guardarAlvoDoServer(endereco) {
  alvoDoServer = endereco || null;
}
/**
 * Os servidores do histórico, como a trilha os lista.
 *
 * A mesma lista da tela de entrada, do mesmo comando e na mesma ordem — do mais
 * recente para o mais antigo, que é a única ordem útil numa lista de atalhos.
 *
 * Lida uma vez por sessão e guardada aqui, e não a cada quadro: o histórico só
 * muda quando alguém entra num servidor ou esquece um, e `desenharTopo` roda
 * duas vezes por segundo. Uma ida ao disco por quadro para uma lista que muda
 * uma vez por dia é o mesmo desperdício que o `icon_revision` existe para
 * evitar, por outra porta.
 */
let conhecidosDaTrilha = [];
/**
 * O que a trilha já tem desenhado, para não reconstruir os botões dela.
 *
 * `null` quer dizer «desenhe de novo». Não é economia de pintura: os botões do
 * histórico são criados por código, e reconstruí-los a cada quadro arrancaria
 * da árvore justamente o que estiver sob o cursor ou com o foco — que é o botão
 * que a pessoa está a ponto de apertar. É o mesmo cuidado que `fecharModeracao`
 * documenta do outro lado, e aqui é a causa em vez do sintoma.
 */
let trilhaDesenhada = null;
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
 * conta o tempo de outro Server.
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
 * Se esta sessão pode tirar a mensagem **de outra pessoa** da Linha.
 *
 * Um campo de módulo e não um parâmetro porque `desenharMensagens` é chamado de
 * três lugares — o snapshot, a busca e a limpeza dela — e só um deles tem um
 * snapshot em mãos. Apagar a **própria** mensagem não consulta isto: não pede
 * permissão nenhuma, e a da `specs/04` diz «de outra pessoa».
 */
let podeRemoverMensagem = false;
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
  nodo.classList.add("ausente");
  nodo.title = motivo;
}

/** O contrário: um valor que existe, no mesmo campo. */
function medido(nodo, texto) {
  nodo.textContent = texto;
  nodo.classList.remove("ausente");
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
  // O servidor é o mesmo de sempre; o link é que não é dele. Ressalva, não
  // queda.
  if (veredito.InviteDisagrees) {
    return (
      "O CONVITE NÃO CORRESPONDE A ESTE SERVIDOR.\n" +
      `esperada: ${veredito.InviteDisagrees.expected}\n` +
      `ofertada: ${veredito.InviteDisagrees.offered}\n` +
      "Você entrou no servidor de sempre. O link é que não leva a ele."
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
  if (comecoDaSessao === null) {
    comecoDaSessao = Date.now();
    // E é onde o histórico é lido: neste ponto o `connect` já anotou o servidor
    // em que se acabou de entrar, então a trilha nasce com ele dentro.
    recarregarTrilha();
  }

  // Antes de qualquer desenho: `desenharMensagens` a lê, e ela chega do
  // servidor como `may_remove_message` — resolvida no PERMISSIONS a partir das
  // permissões desta conexão, e não decidida aqui.
  podeRemoverMensagem = snapshot.may_remove_message === true;

  desenharTopo(snapshot);
  desenharCanais(snapshot);
  desenharOperador(snapshot);
  desenharLinha(snapshot);
  sincronizarMensagens(snapshot.messages_revision);
  desenharPessoas(snapshot);
  desenharTelemetria(snapshot);
  desenharAviso(snapshot);

  desenhado = snapshot;
}

/**
 * O cabeçalho: a marca, o servidor, o estado da conexão, o apelido e o
 * relógio.
 *
 * O bloco do servidor é o `TÓQUIO-3 / SERVER CENTRAL · 7743` do comp (§3.1),
 * com a segunda linha escrita `SERVIDOR · 7743`. Ele mora aqui e em mais lugar
 * nenhum — a ficha `C·02 / SERVER` que já o mostrou era um painel que o comp
 * não desenha, e saiu junto com a trilha voltando.
 */
function desenharTopo(snapshot) {
  const padrao = $("padrao");
  padrao.dataset.padrao = snapshot.pattern;
  // O rótulo diz o estado da conexão em palavras; o `data-padrao` continua
  // sendo o nome do enum, que é por onde a folha escolhe a cor. As três frases
  // são as três do `Pattern` de `client.rs` — desligado, conectado sem
  // verificar, verificado — e nenhuma delas é a cor que a antiga nomeava.
  padrao.textContent = {
    Offline: "SEM CONEXÃO",
    Orange: "CONEXÃO NÃO VERIFICADA",
    Blue: "CONEXÃO SEGURA",
  }[snapshot.pattern];

  $("topo-pessoa").textContent = snapshot.nickname;

  const nome = snapshot.server;
  const rotulo = $("topo-server-nome");
  if (nome) {
    medido(rotulo, nome);
    // O `title` porque o nome pode ser mais largo que o bloco e sair em
    // reticências: o cabeçalho é o único lugar onde ele está por extenso.
    rotulo.title = nome;
  } else {
    // Um servidor sem nome é ele não tendo mandado um, e não um nome vazio.
    naoMedido(rotulo, "este servidor não anunciou nome");
  }

  desenharTrilha(snapshot);
  desenharPortaDoServer();
}

/**
 * `SERVIDOR · 7743` — a segunda linha do bloco do servidor.
 *
 * A porta sai do endereço que esta janela discou, e não do `Snapshot`: o
 * protocolo não carrega para onde nos conectamos, e o inventário §3.5
 * classifica o campo como **S** justamente por isso — "a casca já tem o alvo".
 *
 * Quando o alvo não nomeia porta, a linha fica só `SERVIDOR`. A porta
 * efetiva nesse caso é a padrão do produto (ADR 0005), e escrevê-la aqui seria
 * pôr uma constante de protocolo dentro do JavaScript, que é exatamente o que
 * `specs/06-clientes-gui.md` proíbe. O motivo vai no `title`.
 */
function desenharPortaDoServer() {
  const sub = $("topo-server-sub");
  const porta = /:(\d+)$/.exec(alvoDoServer ?? "");
  if (porta) {
    sub.textContent = `SERVIDOR · ${porta[1]}`;
    sub.removeAttribute("title");
  } else {
    sub.textContent = "SERVIDOR";
    sub.title = "o endereço não nomeou porta; esta sessão está na porta padrão";
  }
}

/**
 * A sigla de um servidor, para a coluna de 60px.
 *
 * `TÓQUIO-3` vira `T3`, como no comp. A primeira letra de cada corrida de
 * letras ou algarismos, até três — e é abreviação de desenho, nunca um dado: o
 * nome inteiro está no cabeçalho e no nome acessível do botão. Uma sigla que
 * fosse a única forma do nome na tela seria informação perdida.
 */
function sigla(nome) {
  const partes = nome.toUpperCase().match(/[\p{L}\p{N}]+/gu);
  if (!partes) return "—";
  return partes
    .map((parte) => [...parte][0])
    .join("")
    .slice(0, 3);
}

/**
 * A sigla de um servidor que só se conhece pelo endereço.
 *
 * O histórico guarda endereço, apelido, último canal e data — e **não** o nome
 * que o servidor anuncia (`crates/seele-core/src/conhecidos.rs`: quatro
 * colunas, num arquivo dividido com o `plug`). Então a coluna de 60px abrevia o
 * que existe, que é o endereço.
 *
 * `sigla` não serve aqui e o motivo é concreto: a primeira letra de cada corrida
 * de `192.168.0.7` dá `110`, três algarismos que não são o endereço e que são os
 * **mesmos** para toda máquina daquela rede — uma abreviação que não distingue é
 * pior que nenhuma. Num IPv4 quem distingue é o último campo, que é como as
 * pessoas leem esses endereços em voz alta; num nome, a primeira etiqueta dele.
 *
 * O endereço inteiro fica no nome acessível do botão, que é o que um leitor de
 * tela anuncia. A sigla é abreviação de desenho, nunca um dado.
 */
function siglaDoAlvo(alvo) {
  // Sem a porta e sem os colchetes de um IPv6: nenhum dos dois identifica o
  // servidor, e os dois comeriam a coluna inteira.
  const host = alvo.replace(/:\d+$/, "").replace(/[[\]]/g, "");
  const ipv4 = /^\d{1,3}\.\d{1,3}\.\d{1,3}\.(\d{1,3})$/.exec(host);
  if (ipv4) return ipv4[1];
  const etiqueta = host.split(".")[0].toUpperCase();
  // Por ponto de código, pela mesma razão que `iniciaisDoApelido`: cortar por
  // índice parte um par substituto ao meio e desenha um caractere de
  // substituição no lugar da inicial.
  return [...etiqueta].slice(0, 3).join("") || SEM_MEDIDA;
}

/**
 * Relê o histórico do disco e redesenha a trilha com ele.
 *
 * Falhar não é motivo para tela nenhuma: a lista de atalhos é conveniência, e o
 * Rust já devolve lista vazia quando o arquivo não abre. O que sobra aqui é a
 * ponte cair, e nesse caso a trilha fica com o servidor atual — que é a coluna
 * como ela era antes desta lista existir.
 */
async function recarregarTrilha() {
  try {
    conhecidosDaTrilha = await invoke("conhecidos");
  } catch (falha) {
    console.warn("conhecidos:", falha);
    return;
  }
  trilhaDesenhada = null;
  if (desenhado) desenharTrilha(desenhado);
}

/**
 * A coluna de 60px: o servidor em que se está, e os do histórico embaixo.
 *
 * O atual é desenhado à parte, e não como o primeiro da lista, porque ele pode
 * não estar nela: um servidor hospedado nesta máquina não é anotado no
 * histórico — `127.0.0.1` não é lugar aonde se volta — e é justamente ele que
 * esta coluna existe para dizer.
 *
 * Ele também sai da lista de baixo. Repeti-lo ali seria oferecer entrar onde já
 * se está, e a troca que isso dispararia é uma sessão derrubada para reabrir a
 * mesma sessão.
 */
function desenharTrilha(snapshot) {
  const nome = snapshot.server;
  const uri = iconeDesenhado.uri;
  const outros = conhecidosDaTrilha.filter((conhecido) => conhecido.alvo !== alvoDoServer);
  // A chave inclui nome e presença de imagem: sem isso, um servidor que ganhou
  // distintivo desde o último desenho não seria redesenhado — a lista teria
  // mudado e a comparação diria que não.
  const chave = outros
    .map((conhecido) => `${conhecido.alvo}|${conhecido.nome ?? ""}|${conhecido.icone ? 1 : 0}`)
    .join("\n");
  if (
    trilhaDesenhada !== null &&
    trilhaDesenhada.nome === nome &&
    trilhaDesenhada.alvo === alvoDoServer &&
    trilhaDesenhada.icone === uri &&
    trilhaDesenhada.outros === chave
  ) {
    return;
  }
  trilhaDesenhada = { nome, alvo: alvoDoServer, icone: uri, outros: chave };

  vestirItemDaTrilha(
    $("trilha-server"),
    nome ? sigla(nome) : SEM_MEDIDA,
    // Um servidor sem nome é ele não tendo mandado um, e não um nome vazio — e
    // um botão sem nome acessível é anunciado pela sigla, que é desenho.
    nome || "servidor sem nome",
    uri,
  );

  repovoar(
    $("trilha-outros"),
    outros.map((conhecido) => {
      const linha = elemento("li");
      const botao = elemento("button", "trilha-item trilha-outro");
      botao.type = "button";
      // O apelido viaja junto porque é com ele que se entra: o histórico guarda
      // com que nome se entrou da última vez em cada servidor, e trocar de
      // servidor sem levá-lo entraria com o apelido de outro lugar.
      botao.dataset.alvo = conhecido.alvo;
      botao.dataset.apelido = conhecido.apelido;
      // **Com nome e imagem**, agora que o histórico os guarda. O comentário
      // aqui dizia o contrário — «o histórico guarda endereços, e a imagem de um
      // servidor só existe enquanto se está dentro dele» — e era verdade e era
      // o defeito: uma coluna de endereços IP não é uma lista que alguém use
      // para escolher para onde voltar. Ninguém decora o IP do amigo.
      //
      // O endereço continua sendo o recuo, e é o certo: uma entrada anotada por
      // uma versão anterior, ou por uma visita pelo terminal, não tem nome.
      const rotulo = conhecido.nome || conhecido.alvo;
      vestirItemDaTrilha(
        botao,
        conhecido.nome ? sigla(conhecido.nome) : siglaDoAlvo(conhecido.alvo),
        // O endereço vai junto do nome no rótulo acessível: dois servidores
        // podem se chamar igual, e é o endereço que os separa.
        conhecido.nome ? `${conhecido.nome} · ${conhecido.alvo}` : conhecido.alvo,
        // `uriDeIcone` de `tela-server.js`, e não uma cópia aqui: escrever
        // `image/png` num segundo arquivo é o que `frontend.rs` proíbe, e com
        // razão. A exceção que ele abre é para **um** lugar compor o tipo, e
        // são os mesmos bytes — os que o protocolo já provou serem PNG nas duas
        // pontas antes de este app os gravar em disco.
        conhecido.icone ? uriDeIcone(conhecido.icone) : null,
      );
      botao.title = rotulo;
      linha.append(botao);
      return linha;
    }),
  );
}

/**
 * Põe num botão da trilha o que couber em 56px, e o nome inteiro por baixo.
 *
 * A imagem ganha da sigla quando existe: numa coluna de ícones, uma imagem se
 * reconhece de relance e três letras se leem. Ela vai com `alt=""` porque o
 * botão já tem nome acessível — uma descrição aqui faria um leitor de tela
 * anunciar o mesmo servidor duas vezes seguidas.
 *
 * O `title` repete o nome acessível de propósito: o que está desenhado no botão
 * é uma abreviação, e quem usa o ponteiro não tem outro lugar onde ler o nome
 * inteiro de um servidor do histórico.
 */
function vestirItemDaTrilha(botao, abreviacao, nomeInteiro, uri) {
  if (uri) {
    const imagem = document.createElement("img");
    imagem.className = "trilha-icone";
    imagem.src = uri;
    imagem.alt = "";
    botao.replaceChildren(imagem);
  } else {
    botao.textContent = abreviacao;
  }
  botao.setAttribute("aria-label", nomeInteiro);
  botao.title = nomeInteiro;
}

/**
 * As iniciais de um apelido, para o avatar das mensagens.
 *
 * `IKARI.S` vira `IS`, como no comp. Sem ponto, as duas primeiras letras;
 * com uma letra só, essa letra. Por ponto de código e não por índice de unidade
 * de código — um apelido que comece com emoji devolveria meio par substituto,
 * que é um caractere de substituição desenhado no lugar da inicial.
 *
 * É desenho e nunca dado: o nome inteiro está sempre ao lado, e o avatar sai
 * `aria-hidden` por isso mesmo.
 *
 * `iniciaisDoApelido` e não `iniciais`, que é o nome do comp: os scripts desta
 * janela dividem **um** escopo global (ADR 0019, e o topo de `base.js`), e a
 * tela de chamada desenha o mesmo avatar por cartão. Duas declarações de
 * `function iniciais` no mesmo escopo não são erro em lugar nenhum — a última
 * carregada simplesmente vence, e as duas telas passam a usar a regra de uma
 * delas. Quando a segunda existir, o lugar deste desenho é `base.js`, como o
 * `relogio` e o `marcaSync` que já moram lá.
 */
function iniciaisDoApelido(apelido) {
  const partes = apelido.split(".").filter((parte) => parte.length > 0);
  const primeira = [...(partes[0] ?? "")];
  const segunda = [...(partes[1] ?? "")];
  return ((primeira[0] ?? "") + (segunda[0] ?? primeira[1] ?? "")).toUpperCase();
}

/**
 * Os VoiceRooms e as Linhas, em duas listas com cabeçalho próprio.
 *
 * ---- ver quem está dentro antes de entrar ----
 *
 * A mudança de fundo do v3 (§6.2), e ela não custa protocolo nenhum: `voice_rooms_of`
 * popula `VoiceRoom.people` a partir de `room.roster(voice_room.id)` para **todo** VoiceRoom, e
 * não só para o ocupado. O produto sabia quem estava em cada sala e desenhava
 * uma barra de blocos no lugar dos nomes.
 *
 * ---- entrar é um botão, e sair também ----
 *
 * No v2 a linha inteira era o alvo do clique, sem rótulo e sem foco de teclado
 * — o ouvinte estava no `<ul>` e nenhum `<li>` era apertável. Agora cada VoiceRoom
 * traz um botão de 38px que diz o que faz.
 *
 * O comp escreve `VOCÊ ESTÁ AQUI` no VoiceRoom ocupado e não liga o botão a nada.
 * Aqui ele diz `SAIR DA SALA` e ejeta, e a divergência é deliberada: `sair`
 * ganhou lugar próprio no v3 justamente porque no v2 ninguém o achava, e o
 * único outro lugar em que ele existe é a tela de chamada. Trocar um botão
 * mudo por um botão morto seria repetir o erro que o v3 corrige. Que se está
 * dentro continua dito, e por duas vias: a marca laranja na borda do VoiceRoom e o
 * `(você)` ao lado do próprio nome na lista de quem está lá.
 */
function desenharCanais(snapshot) {
  const voice_rooms = snapshot.voice_rooms.map((voice_room) => {
    const item = elemento("li", voice_room.occupied_by_us ? "voice room aberto" : "voice room");

    const cabeca = elemento("span", "canal-cabeca");
    cabeca.append(elemento("span", "voice_room-nome", voice_room.name));
    // `4/8` — a ocupação em número. Ela não acompanha mais uma barra de blocos:
    // a lista de nomes logo abaixo é a mesma informação com os nomes dentro,
    // e duas leituras da mesma coisa numa coluna de 268px é uma a mais.
    cabeca.append(elemento("span", "voice_room-ocupacao", `${voice_room.people.length}/${voice_room.limit}`));

    const dentro = elemento("ul", "voice_room-dentro");
    if (voice_room.people.length === 0) {
      // Palavra, e não travessão: uma sala de voz vazio é uma medida, e o produto a
      // tem. O travessão é para o que ninguém mediu.
      dentro.append(elemento("li", "voice_room-vazio", "ninguém aqui"));
    } else {
      dentro.append(...voice_room.people.map((pessoa) => linhaDeQuemEstaDentro(pessoa, snapshot)));
    }

    const entrar = elemento(
      "button",
      "voice_room-entrar",
      voice_room.occupied_by_us ? "SAIR DA SALA" : "ENTRAR NA SALA",
    );
    entrar.type = "button";
    entrar.dataset.voice_room = String(voice_room.id);
    entrar.dataset.dentro = voice_room.occupied_by_us ? "sim" : "nao";
    entrar.title = voice_room.occupied_by_us
      ? "sair: você para de ouvir e de falar nesta sala"
      : `entrar e falar com quem está em ${voice_room.name}`;

    // Os dois botões da sala numa fileira só. Com um deles ausente — que é o
    // caso de quem não administra o servidor — a fileira tem um filho de
    // `flex: 1`, e sai idêntica ao botão de largura cheia de antes.
    const botoes = elemento("div", "voice_room-botoes");
    botoes.append(entrar);
    // O último sala de voz vem desabilitado e não escondido: a razão de ele não poder
    // ir embora é coisa que se lê, e uma ausência não se lê.
    const apagar = botaoDeApagarVoiceRoom(voice_room, snapshot, snapshot.voice_rooms.length === 1);
    if (apagar) botoes.append(apagar);

    item.append(cabeca, dentro, botoes);
    return item;
  });
  repovoar($("lista-voice_rooms"), voice_rooms);

  const linhas = snapshot.channels.map((linha) => {
    const item = elemento("li", null);

    // Botão de verdade, e não um `<li>` com `cursor: pointer`: o ouvinte estava
    // no `<ul>`, então a lista inteira era inalcançável pelo teclado e nenhum
    // leitor de tela a anunciava como algo que se aperta.
    const botao = elemento("button", linha.open ? "linha aberto" : "linha");
    botao.type = "button";
    botao.dataset.linha = String(linha.id);
    if (linha.open) botao.setAttribute("aria-current", "true");

    // O `#` é ASCII e está na face, então é caractere e não desenho. Decoração
    // ao lado do nome, e por isso sem nome acessível.
    const cerquilha = elemento("span", "linha-cerquilha", "#");
    cerquilha.setAttribute("aria-hidden", "true");
    botao.append(cerquilha, elemento("span", "linha-rotulo", linha.name));

    // A contagem de pendências do comp não entra. `Channel` é `{id, name, open}` —
    // não há contagem de não-lidas nem marca d'água de leitura em lugar nenhum
    // do core —, e um travessão explicado ao lado de cada Linha seria meia
    // dúzia de perguntas que ninguém fez, numa tela que existe para ser
    // simples. Ver o cabeçalho de `tela-sessao.css`.

    item.append(botao);
    // Irmão do botão, e não filho: um `<button>` dentro de outro não é
    // marcação válida, e o alvo do clique de abrir a Linha é a fileira inteira.
    const apagar = botaoDeApagarLinha(linha, snapshot);
    if (apagar) {
      item.className = "linha-fileira";
      item.append(apagar);
    }
    if (linha.open) linhaAberta = linha.id;
    return item;
  });
  repovoar($("lista-linhas"), linhas);

  // Os dois formulários de criar sala só aparecem para quem pode criar. Quem
  // responde é o servidor, em `may_manage_voice_rooms`, resolvido pelo PERMISSIONS a
  // partir das permissões desta conexão. Esconder aqui não impede ninguém de
  // nada — `CreateVoiceRoom` de quem não tem `ManageVoiceRooms` é recusado lá, e há teste
  // de conformidade provando que a recusa é de lá. Isto é não oferecer o que
  // não ia funcionar.
  const pode = snapshot.may_manage_voice_rooms === true;
  $("criar-voice_room").hidden = !pode;
  $("criar-linha").hidden = !pode;

  // O tamanho padrão da sala nova vem de uma sala que já existe, e não de um
  // número escrito no JavaScript: quem hospeda já disse que tamanho quer
  // quando montou o servidor, e repetir a escolha dele é mais honesto que
  // inventar quinze. Só enquanto o campo estiver como a marcação o deixou.
  const lugares = $("campo-voice_room-limite");
  if (pode && lugares.value === lugares.defaultValue && snapshot.voice_rooms.length > 0) {
    lugares.value = String(snapshot.voice_rooms[0].limit);
  }
}

/**
 * Uma pessoa na lista de quem está dentro de um VoiceRoom.
 *
 * O comp marca o estado com um ponto colorido e nada mais.
 * `specs/05-cliente-tui.md` proíbe informação que só a cor carregue, e um ponto
 * é também só forma — então o que muda o estado vira **palavra**: `fala` e
 * `mudo`. O repouso não ganha palavra nenhuma, pela mesma razão que a pastilha
 * `EM ESCUTA` do roster não ganha bloco: é onde toda linha está quase sempre, e
 * marcá-lo é não marcar nada.
 *
 * O glifo continua, desenhado e `aria-hidden`: ele é o que dá a varredura da
 * coluna de relance, e quem escuta a tela já recebe a palavra.
 */
function linhaDeQuemEstaDentro(pessoa, snapshot) {
  const item = elemento("li", "voice_room-pessoa");
  item.append(glifo(pessoa.speaking ? "falando" : "silencio"));
  item.append(elemento("span", "voice_room-pessoa-nome", pessoa.nickname));
  if (pessoa.is_self) item.append(elemento("span", "voice_room-pessoa-eu", "(você)"));
  if (pessoa.at_field) {
    item.append(elemento("span", "voice_room-pessoa-marca", "mudo"));
  } else if (pessoa.speaking) {
    item.append(elemento("span", "voice_room-pessoa-marca", "fala"));
  }
  // A porta da moderação, quando esta sessão tem algum verbo sobre gente.
  // `camada-moderar.js` decide se há o que oferecer.
  //
  // Ela continua morando só aqui. A faixa de pessoas passou a listar todo mundo
  // em toda sala também — era a razão de esta lista ser a única com a porta —,
  // mas duas portas para o mesmo verbo é uma a mais, e a que existe é a que já
  // tem teste. Mover a porta para a faixa, ou dar uma a ela, é decisão de quem
  // coordena: o comentário de `botaoDeModerar` em `camada-moderar.js` ainda diz
  // que o roster mostra só a sala ocupada, e ele tem de mudar junto.
  const porta = botaoDeModerar(pessoa, snapshot);
  if (porta) item.append(porta);
  return item;
}

/** A tira do operador, no rodapé da coluna de canais. */
function desenharOperador(snapshot) {
  $("operador-nome").textContent = snapshot.nickname;

  // O botão diz em que estado o microfone está, e não o que apertá-lo vai
  // fazer. Um botão escrito com o verbo é um botão que ninguém sabe ler quando
  // volta a olhar para a tela.
  const mudo = $("botao-mudo");
  mudo.textContent = snapshot.at_field ? "MICROFONE FECHADO" : "MICROFONE ABERTO";
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
  // `PODE FALAR` e não `MICROFONE ABERTO` no modo aberto: o botão de mudo ao
  // lado passou a dizer exatamente essa frase, e dois controles vizinhos com o
  // mesmo rótulo são dois controles que ninguém distingue. O rótulo daqui é a
  // instrução; o de lá é o estado.
  $("falar-rotulo").textContent = snapshot.speaking
    ? "NO AR"
    : { PushToTalk: "SEGURE ESPAÇO", VoiceActivated: "FALE", Open: "PODE FALAR" }[
        snapshot.voice_mode
      ] ?? "SEGURE ESPAÇO";
  falar.disabled = !snapshot.audio_available;
  falar.title = snapshot.audio_available
    ? "segure a barra de espaço, ou este botão"
    : "esta sessão não tem áudio";
}

/**
 * A barra do canal aberto: `#` e o nome, como no comp v3.
 *
 * `LINHA geral` virou `# geral`: a palavra estava dizendo o que o `#` ao lado
 * já dizia, e o cabeçalho da coluna tem um botão a mais agora.
 *
 * Sem canal aberto a frase é uma frase, e não um travessão: nenhum canal
 * aberto é um estado que o produto conhece e sabe nomear, ao contrário de um
 * campo que ninguém mediu.
 */
function desenharLinha(snapshot) {
  const aberta = snapshot.channels.find((linha) => linha.open);
  const nome = $("linha-nome");
  nome.textContent = aberta ? aberta.name : "nenhum canal aberto";
  nome.classList.toggle("linha-nome-vazio", !aberta);
}

/**
 * A conversa que esta tela tem em mãos, e qual revisão ela é.
 *
 * O `snapshot` não a carrega mais. Ele carregava, e o preço era clonar em Rust
 * cada apelido e cada corpo já ditos, serializar tudo em JSON e reconstruir
 * todos os nós do DOM — a cada 500 ms e a cada evento. O custo crescia com a
 * conversa, então uma sessão longa ficava lenta de escrever. Apareceu num teste
 * entre duas máquinas, que é onde uma conversa dura o bastante.
 *
 * Agora o snapshot diz só um número, e este módulo busca a lista quando ele
 * muda. `seele_core::Changed` já sabia disto desde sempre — a documentação dele
 * diz que uma casca que compara dois snapshots para descobrir que chegou
 * mensagem é uma casca refazendo o trabalho do core. Era o que esta fazia.
 */
let mensagens = [];
let revisaoDesenhada = null;
let buscandoMensagens = false;

/**
 * Busca e redesenha a conversa, se ela mudou desde a última vez.
 *
 * A guarda contra chamadas simultâneas importa: o tique de 500 ms e o evento de
 * mensagem chegam por caminhos diferentes e podem se cruzar, e duas buscas em
 * voo escreveriam a lista duas vezes — a segunda possivelmente com dados mais
 * velhos que a primeira.
 */
async function sincronizarMensagens(revisao) {
  if (revisao === revisaoDesenhada || buscandoMensagens) return;
  buscandoMensagens = true;
  try {
    mensagens = await invoke("messages");
    revisaoDesenhada = revisao;
    desenharMensagens();
  } catch (erro) {
    if (erro !== "NotConnected") console.warn("messages:", erro);
  } finally {
    buscandoMensagens = false;
  }
}

/**
 * Quanto tempo cabe entre duas mensagens da mesma pessoa antes de a segunda
 * deixar de ser continuação da primeira.
 *
 * Quinze minutos, e o número é o que separa uma rajada de uma volta. A mesma
 * pessoa retomando o assunto depois do almoço não é a mesma fala continuando: o
 * cabeçalho único diria que aquele bloco todo aconteceu na hora do primeiro.
 */
const JUNTA_ATE_SEGUNDOS = 900;

/**
 * O dia local de uma mensagem, como chave de comparação. `null` sem hora.
 *
 * Ano, mês e dia da máquina de quem lê, e não do relógio de quem escreveu: o
 * divisor responde «que dia era para mim», que é a pergunta de quem está
 * rolando a conversa para trás.
 */
function diaDaMensagem(segundos) {
  if (!segundos) return null;
  const quandoFoi = new Date(segundos * 1000);
  return `${quandoFoi.getFullYear()}-${quandoFoi.getMonth()}-${quandoFoi.getDate()}`;
}

/**
 * O que o divisor de dia escreve.
 *
 * `HOJE` e `ONTEM` por extenso, e a data nos outros: são as duas respostas que
 * dispensam a conta, e são as duas que quem rola para trás faz o tempo todo.
 * A conta é entre meias-noites locais, e não em múltiplos de 24 horas — às
 * nove da manhã, «ontem às vinte e duas» está a onze horas, e onze horas
 * arredondadas para baixo dariam «hoje».
 */
function rotuloDoDia(segundos) {
  const data = new Date(segundos * 1000);
  const meiaNoite = (d) => new Date(d.getFullYear(), d.getMonth(), d.getDate()).getTime();
  const dias = Math.round((meiaNoite(new Date()) - meiaNoite(data)) / 86400000);
  if (dias === 0) return "HOJE";
  if (dias === 1) return "ONTEM";
  return data
    .toLocaleDateString([], { day: "2-digit", month: "2-digit", year: "numeric" })
    .toUpperCase();
}

/** A faixa que separa dois dias de conversa. */
function divisorDeDia(segundos) {
  // Item da lista, e não `aria-hidden`: a data é informação e quem escuta a
  // tela também está perguntando de que dia é o que vem a seguir. Os traços
  // dos dois lados são da folha, em `::before`/`::after`, e por isso não
  // chegam a leitor nenhum.
  return elemento("li", "mensagens-dia", rotuloDoDia(segundos));
}

/**
 * Uma mensagem da conversa.
 *
 * `segue` diz que esta continua a rajada da anterior — mesma pessoa, mesmo
 * dia, e perto no relógio. Nesse caso o avatar e o nome não são repetidos: um
 * nome que se repete seis vezes seguidas é seis leituras para descobrir que
 * ninguém mudou. **A hora não some junto**: ela desce para a calha de 34px do
 * avatar, porque uma hora que só aparece ao passar o ponteiro é uma hora que
 * não está na tela, e este produto já gastou uma tela inteira desfazendo essa
 * lição.
 */
function itemDeMensagem(mensagem, indice, segue) {
  // A grade do comp v3: 34px de avatar, o resto para autor, hora e corpo. A
  // hora saiu da coluna própria de 76px e foi para o lado do nome — é o que
  // devolveu a largura que o avatar ocupa, e é onde o comp a põe.
  //
  // A marca de 2px à esquerda é onde o comp distingue mensagem de sistema e
  // de alerta — `Message` não tem tipo (inventário §16), então só duas
  // larguras existem aqui: a própria e a dos outros.
  const item = elemento("li", mensagem.own ? "mensagem propria" : "mensagem");
  if (segue) item.classList.add("mensagem-segue");
  // O índice viaja no nó, e é por isso que `desenharBusca` não conta filhos.
  //
  // `setAttribute` e não `dataset`: escrita com ponto, esta propriedade fica
  // com a mesma sequência de caracteres que `tests/frontend.rs` proíbe em
  // qualquer script — o nome em português de um campo que um `Match`
  // serializa, que ali chegaria como `undefined` e pintaria realce vazio sem
  // erro nenhum. O guarda é literal e está certo em ser; quem desvia é esta
  // linha.
  item.setAttribute("data-mensagem", String(indice));

  if (segue) {
    // A calha de 34px do avatar recebe a hora quando o avatar não vem. É o que
    // mantém toda linha datada sem repetir o nome seis vezes seguidas — e o
    // que impede a solução comum, que é esconder a hora até alguém passar o
    // ponteiro por cima dela.
    item.append(
      elemento("span", "mensagem-hora mensagem-hora-calha", relogio(mensagem.at_seconds)),
    );
  } else {
    // O avatar de iniciais. Desenho e não dado: o nome inteiro está a doze
    // pixels dali, então ele sai `aria-hidden` — anunciar `KM` antes de
    // `KATSURAGI.M` é ler a mesma coisa duas vezes, uma delas em código.
    //
    // O `m.selo` do comp, ao lado do autor, **não** entra: ver o §1.2 do
    // inventário v3 e a frase no rodapé da coluna de canais.
    const avatar = elemento("span", "mensagem-avatar", iniciaisDoApelido(mensagem.author_nickname));
    avatar.setAttribute("aria-hidden", "true");
    item.append(avatar);
  }

  const conteudo = elemento("span", "mensagem-conteudo");
  const cabeca = elemento("span", "mensagem-cabeca");
  if (!segue) {
    cabeca.append(elemento("span", "mensagem-autor", mensagem.author_nickname));
    cabeca.append(elemento("span", "mensagem-hora", relogio(mensagem.at_seconds)));
  }
  if (mensagem.edited) cabeca.append(elemento("span", "editada", "editada"));
  // Tirar a mensagem do canal, quando esta sessão pode tirar esta mensagem.
  // Desenhado e nunca revelado pelo ponteiro: um controle que só existe no
  // hover é um controle escondido, e o v3 gastou uma tela inteira desfazendo
  // essa lição. `camada-moderar.js` decide se há o que oferecer, e pergunta
  // primeiro se a mensagem é da própria pessoa — essa não pede permissão.
  //
  // Numa continuação sem edição e sem botão o cabeçalho não tem o que dizer, e
  // aí ele não é escrito: uma linha vazia por mensagem devolveria o espaço que
  // agrupar acabou de poupar.
  const remover = botaoDeRemoverMensagem(mensagem, podeRemoverMensagem);
  if (remover) cabeca.append(remover);
  if (cabeca.childElementCount > 0) conteudo.append(cabeca);

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
  // O arquivo, embaixo do texto. ADR 0027: o texto sobrevive ao arquivo, e é
  // por isso que este bloco continua aparecendo depois de os bytes irem
  // embora — com o nome, o tamanho, e a frase que diz o que houve.
  if (mensagem.attachment) conteudo.append(blocoDeAnexo(mensagem.attachment));

  item.append(conteudo);
  return item;
}

/**
 * Escreve a conversa inteira: um cabeçalho por rajada, um divisor por dia.
 *
 * A lista não é mais uma mensagem por linha da lista — ela tem dois tipos de
 * filho —, e é por isso que cada mensagem leva o próprio índice num
 * `data-mensagem`. `desenharBusca` alcançava a mensagem pelo lugar dela entre
 * os filhos, e o primeiro divisor desalinharia essa conta em um, calado, para
 * todas as mensagens do dia seguinte.
 */
function desenharMensagens() {
  const lista = $("lista-mensagens");

  // Sem layout não há o que redesenhar, e insistir corrompe a leitura de quem
  // volta. A chamada e o Terminal server abrem por cima da sessão, e um evento
  // que chegue com uma delas aberta cai aqui com a lista em `display: none` —
  // onde `scrollHeight`, `scrollTop` e `clientHeight` valem todos 0. A conta
  // abaixo vira `0 - 0 - 0 < 32`, isto é, "estava no fim", para justamente
  // quem tinha subido para ler. O `repovoar` seguinte ainda troca todos os
  // filhos, o que zera a rolagem de qualquer forma.
  //
  // A lista é redesenhada ao voltar — `fecharChamada` e `fecharServer` pedem
  // isso —, então sair daqui não deixa nada velho na tela.
  if (lista.clientHeight === 0) return;

  // Só rola sozinho se já estava no fim: puxar alguém de volta para baixo no
  // meio de uma leitura é pior do que não acompanhar.
  const noFim = lista.scrollHeight - lista.scrollTop - lista.clientHeight < 32;

  const itens = [];
  let anterior = null;
  mensagens.forEach((mensagem, indice) => {
    const dia = diaDaMensagem(mensagem.at_seconds);
    const trocouDeDia = dia !== null && dia !== diaDaMensagem(anterior?.at_seconds);
    if (trocouDeDia) itens.push(divisorDeDia(mensagem.at_seconds));
    // Sem hora dos dois lados não há como saber se a rajada é a mesma, e o
    // desconhecido abre cabeçalho: repetir o nome custa uma linha, e engolir
    // o nome de quem falou custa a autoria.
    const segue =
      anterior !== null &&
      !trocouDeDia &&
      anterior.author_nickname === mensagem.author_nickname &&
      Boolean(anterior.at_seconds) &&
      Boolean(mensagem.at_seconds) &&
      mensagem.at_seconds - anterior.at_seconds <= JUNTA_ATE_SEGUNDOS;
    itens.push(itemDeMensagem(mensagem, indice, segue));
    anterior = mensagem;
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
 * noutra mensagem. Sem ele todas as ocorrências saíam idênticas e o pessoa não
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

// ------------------------------------------------------------------ pessoas

/**
 * A faixa da direita: quem está aqui, agrupado por sala.
 *
 * Ela era a sincronia — a média da sala em 52px e uma linha por pessoa da sala
 * ocupada. A avaliação de usabilidade chamou aquilo de o pior erro de alocação
 * da tela, e com razão: 328px permanentes de uma ferramenta de comunicação
 * gastos num dado de diagnóstico, com a lista de pessoas sem aparecer em tela
 * nenhuma fora da coluna de 268px. A média desceu para o rodapé, que é onde a
 * telemetria mora, e as pessoas subiram para a faixa.
 *
 * ---- o alcance desta lista, que é menor do que o nome dela ----
 *
 * `snapshot.voice_rooms` traz `people` para **todo** VoiceRoom e não só para o ocupado, e
 * é isso que esta lista percorre: todo mundo que está em alguma sala de voz.
 *
 * Quem está conectado no servidor **sem** estar numa sala não está aqui, e não
 * está porque o protocolo não o carrega: `Room.people` só cresce com o
 * `Session` desta conexão, com `PersonJoined` — que anuncia a entrada numa sala
 * e traz o VoiceRoom no próprio campo — e com o autor de uma mensagem. Não há
 * mensagem na fita que diga quem entrou no servidor e ficou fora das salas.
 * Enquanto não houver, a nota do cabeçalho diz de que a lista é, e esta função
 * não inventa o resto.
 *
 * ---- por que agrupado ----
 *
 * Sem grupo, uma dúzia de nomes seguidos não diz de onde nenhum deles fala, e a
 * informação nova que a faixa passou a carregar — que há gente noutras salas —
 * seria justamente a que se perderia. O cabeçalho do grupo é o nome da sala com
 * a ocupação ao lado, que é o mesmo par que a coluna de salas escreve.
 */
function desenharPessoas(snapshot) {
  const nossa = snapshot.voice_rooms.find((c) => c.occupied_by_us);

  const grupos = snapshot.voice_rooms
    .filter((voice_room) => voice_room.people.length > 0)
    .map((voice_room) =>
      grupoDeSala(
        voice_room.name,
        `${voice_room.people.length}/${voice_room.limit}`,
        voice_room.occupied_by_us,
        voice_room.people.map((pessoa) =>
          linhaDoRoster(
            {
              nome: pessoa.nickname + (pessoa.is_self ? " (você)" : ""),
              ratio: pessoa.sync_ratio,
              faixa: pessoa.sync_band,
              falando: pessoa.speaking,
              atField: pessoa.at_field,
              isolado: pessoa.total_isolation,
              // O deslizante é dos outros: baixar o próprio volume não faz
              // nada, porque a própria voz nunca entra na mistura
              // (`specs/03-audio.md`).
              volume: pessoa.is_self ? null : pessoa.nickname,
            },
            snapshot.audio_available,
          ),
        ),
      ),
    );

  // Fora de sala, o próprio operador ganha um grupo com o nome do lugar onde
  // ele está. O sinal é a medida que `specs/05-cliente-tui.md` chama de
  // permanente, e sumir com ela porque ainda não se entrou em sala nenhuma
  // seria escondê-la justo enquanto se decide em qual entrar. O rótulo do grupo
  // é o mesmo `FORA DE SALA` que o rodapé escreve, e não um grupo sem nome:
  // um cartão solto no alto da lista pareceria pertencer à primeira sala.
  if (!nossa) {
    grupos.unshift(
      grupoDeSala("FORA DE SALA", null, false, [
        linhaDoRoster(
          {
            nome: `${snapshot.nickname} (você)`,
            ratio: snapshot.telemetry.sync_ratio,
            faixa: snapshot.telemetry.sync_band,
            falando: snapshot.speaking,
            atField: snapshot.at_field,
            isolado: snapshot.total_isolation,
            volume: null,
          },
          snapshot.audio_available,
        ),
      ]),
    );
  }

  // Quem está conectado e fora de toda sala.
  //
  // Esta lista não existia, e a falta dela era o que fazia o sincronismo
  // parecer estranho: `PersonJoined` carrega uma sala de voz porque anuncia sentar-se
  // num, e não havia mensagem para estar aqui — quem entrava no servidor e não
  // escolhia sala nenhuma era invisível para todo mundo. `PersonPresent` e
  // `PersonGone` fecharam isso do lado do protocolo; esta é a metade que
  // aparece.
  //
  // Uma subtração e não uma terceira lista: `presentes` é todo mundo, as salas de voz
  // dizem quem está sentado, e quem sobra está no saguão. O próprio operador
  // sai daqui quando não está em sala porque ele já tem o grupo `FORA DE SALA`
  // logo acima, com a telemetria que só a própria sessão mede.
  const sentados = new Set(
    snapshot.voice_rooms.flatMap((voice_room) => voice_room.people.map((pessoa) => pessoa.id)),
  );
  const noSaguao = snapshot.presentes.filter(
    (pessoa) => !sentados.has(pessoa.id) && !pessoa.is_self,
  );
  if (noSaguao.length > 0) {
    grupos.push(
      grupoDeSala(
        "NO SERVIDOR, FORA DAS SALAS",
        `${noSaguao.length}`,
        false,
        noSaguao.map((pessoa) =>
          linhaDoRoster(
            {
              nome: pessoa.nickname,
              // Sem medida: o sinal de quem não está numa sala não é medido por
              // ninguém — não há voz atravessando para medir. Travessão é o que
              // este produto escreve onde não mediu, e inventar zero aqui seria
              // desenhar uma barra vermelha para quem está bem.
              ratio: null,
              faixa: null,
              falando: false,
              atField: pessoa.at_field,
              isolado: pessoa.total_isolation,
              volume: null,
            },
            snapshot.audio_available,
          ),
        ),
      ),
    );
  }

  repovoar($("lista-roster"), grupos);

  // Duas contagens, e a de fora existe porque a de dentro sozinha mentia por
  // omissão: `12 EM SALAS DE VOZ` ao lado de `PESSOAS` era lido como a
  // população do servidor, e não era.
  const emSalas = snapshot.voice_rooms.reduce((soma, voice_room) => soma + voice_room.people.length, 0);
  const total = snapshot.presentes.length;
  medido(
    $("pessoas-conta"),
    total > emSalas ? `${emSalas} EM SALAS · ${total} NO SERVIDOR` : `${emSalas} EM SALAS DE VOZ`,
  );
}

/** Um grupo da faixa: o nome de uma sala e os cartões de quem está nela. */
function grupoDeSala(nome, ocupacao, nossa, cartoes) {
  const grupo = elemento("li", "roster-grupo");
  if (nossa) grupo.dataset.nossa = "sim";

  const cabeca = elemento("h3", "roster-sala");
  cabeca.append(elemento("span", "roster-sala-nome", nome));
  if (ocupacao !== null) cabeca.append(elemento("span", "roster-sala-conta", ocupacao));
  // Que se está nesta sala, em palavra e não só na marca da borda: a borda é
  // cor, e `specs/05-cliente-tui.md` não aceita informação que só a cor carregue.
  if (nossa) cabeca.append(elemento("span", "roster-sala-aqui", "você está aqui"));

  const lista = elemento("ul", "roster-pessoas");
  lista.append(...cartoes);

  grupo.append(cabeca, lista);
  return grupo;
}

/**
 * A média de sinal da sala ocupada, no rodapé.
 *
 * Ela **não é calculada aqui**. Chega em `voice_room.sync` já com faixa e decidida
 * uma vez no core — `types.rs` argumenta que duas cascas com duas cópias de
 * "85 é nominal" são duas cascas que discordam no dia em que uma delas for
 * atualizada, e o comp faz exatamente essa cópia (`corSync(media)` na casca).
 * `null` quando a sala está vazia: uma sala sem ninguém não tem média, e zero
 * pintaria toda sala parada de vermelho.
 *
 * O travessão fica, e é a metade da regra de omissão do v3 que esta tela não
 * inverteu: aqui a ausência responde a uma pergunta que o rótulo ao lado acabou
 * de fazer, e o `title` diz o que falta para haver número.
 */
function desenharMedia(voice_room) {
  const celula = $("tel-sinal");
  const valor = $("tel-sinal-valor");
  const marca = $("tel-sinal-marca");

  if (!voice_room || !voice_room.sync) {
    // Sem sala, ou numa sala vazia. Não é uma média baixa: é a ausência de
    // qualquer coisa para tirar média de.
    delete celula.dataset.faixa;
    marca.textContent = "";
    naoMedido(valor, voice_room ? "esta sala está vazia" : "você não entrou em nenhuma sala");
    return;
  }

  const sync = voice_room.sync;
  celula.dataset.faixa = sync.band;
  // A marca de bloco é a metade que sobrevive sem cor. Ela fica em face
  // monoespaçada ao lado do número porque a Saira Condensed, que desenha o
  // número, não tem `U+2588`.
  marca.textContent = marcaSync(sync.band);
  medido(valor, String(sync.ratio));
}

/**
 * O cartão de uma pessoa.
 *
 * Três faixas de informação, e todas as três têm acompanhante textual: o
 * número ao lado da marca de bloco, a barra de 20 blocos, e a pastilha de
 * estado em palavra. Nenhuma delas depende de enxergar a cor
 * (`specs/05-cliente-tui.md`), e é essa a propriedade que a mudança de coluna
 * tinha de preservar inteira — ela veio junto, sem uma linha de diferença.
 */
function linhaDoRoster(pessoa, temAudio) {
  // `ratio: null` é «ninguém mediu isto», e não zero.
  //
  // Quem está conectado e fora de toda sala não tem sinal medido: não há voz
  // atravessando para medir. Um zero aqui desenharia a barra vermelha do sinal
  // crítico para quem está perfeitamente bem — e o travessão é o que este
  // produto escreve onde não mediu, em toda outra tela.
  const medido = pessoa.ratio !== null && pessoa.ratio !== undefined;

  const item = elemento("li", pessoa.falando ? "pessoa falando" : "pessoa");
  if (medido) item.dataset.faixa = pessoa.faixa;
  else item.dataset.semMedida = "sim";

  const cabeca = elemento("span", "pessoa-cabeca");
  const identidade = elemento("span", "pessoa-identidade");
  identidade.append(elemento("span", "pessoa-nome", pessoa.nome));

  // `PERMISSIONS·01`, o subsistema por pessoa, não entra. O protocolo não diz qual
  // atende quem, e um travessão explicado em toda linha do roster é o ruído que
  // o v3 veio tirar desta tela.

  const numero = elemento("span", "pessoa-sync");
  // A marca de bloco antes do número, pela mesma razão que na média: a Saira
  // desenha o número e não tem o bloco.
  if (medido) {
    numero.append(
      elemento("span", "sync-marca", marcaSync(pessoa.faixa)),
      // Inteiro, e não `98.4`: `sync_ratio` é `u8` em todo ponto onde existe, e
      // uma casa decimal aqui seria precisão inventada no último passo.
      elemento("span", "pessoa-sync-valor", String(pessoa.ratio)),
    );
  } else {
    numero.append(elemento("span", "pessoa-sync-valor", SEM_MEDIDA));
  }

  cabeca.append(identidade, numero);

  const barra = elemento("span", "barra", medido ? blocos(pessoa.ratio, 20) : "");
  barra.setAttribute("aria-hidden", "true");

  // O `ATRASO` por pessoa do comp não entra. `Telemetry` é a **nossa** conexão:
  // `rtt_ms` é um número só, e latência por par não atravessa a fronteira nem é
  // derivável de nada que atravesse (inventário v3 §1.3). O rodapé fica com o
  // que existe.
  const rodape = elemento("span", "pessoa-rodape");
  const estados = elemento("span", "pessoa-estados");
  // A pastilha do comp: bloco sólido com texto no negro absoluto, e não texto
  // colorido. `PLUG EJETADO` é o quarto estado do comp e não aparece aqui —
  // quem sai some de `voice_room.people`, e manter a lápide exigiria ou um campo de
  // estado no `Person`, ou esta casca lembrando de quem estava ali, que é
  // exatamente o estado derivado que o topo de `base.js` proíbe.
  const estado = pessoa.atField
    ? "MUDO"
    : pessoa.falando
      ? "TRANSMITINDO"
      : "EM ESCUTA";
  const pastilha = elemento("span", "pastilha", estado);
  pastilha.dataset.estado = pessoa.atField ? "at" : pessoa.falando ? "fala" : "escuta";
  estados.append(pastilha);

  // O isolamento total não existe no comp e existe no produto. Segunda
  // pastilha, e não uma troca da primeira: estar surdo e estar transmitindo são
  // dois fatos ao mesmo tempo, e um deles apagando o outro esconderia metade.
  if (pessoa.isolado) {
    const surdez = elemento("span", "pastilha", "ISOLAMENTO TOTAL");
    surdez.dataset.estado = "surdo";
    estados.append(surdez);
  }

  rodape.append(estados);
  item.append(cabeca, barra, rodape);

  // Volume por pessoa (`specs/03-audio.md`).
  if (pessoa.volume !== null && temAudio) {
    const volume = document.createElement("input");
    volume.type = "range";
    volume.className = "volume";
    volume.min = "0";
    volume.max = "200";
    volume.step = "10";
    volume.value = String(volumes.get(pessoa.volume) ?? 100);
    volume.title = `volume de ${pessoa.volume}`;
    volume.dataset.pessoa = pessoa.volume;
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

  const voice_room = snapshot.voice_rooms.find((c) => c.occupied_by_us);
  medido($("tel-voice_room"), voice_room ? voice_room.name : "FORA DE SALA");

  // A média da sala é vizinha do nome dela, e as duas saem do mesmo achado: a
  // sala em que se está e como ela vai são a mesma pergunta em duas metades.
  desenharMedia(voice_room);

  // Escrito uma vez e calado depois. O caminho não muda dentro de uma sessão —
  // a reconexão do núcleo volta ao mesmo endereço —, e reescrever o mesmo texto
  // duas vezes por segundo seria movimento sem informação, que é o que
  // `specs/07-tema-evangelion.md` chama de falha de design.
  //
  // Sem nome, nada é escrito e o travessão fica: é a métrica dizendo que não
  // sabe, e não a tela inventando um «DIRETO» que apagaria a distinção que
  // importa.
  const caminho = fraseDeCaminho(snapshot.caminho);
  if (caminho !== null && $("tel-caminho").textContent !== caminho) {
    medido($("tel-caminho"), caminho);
  }

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
  // O segundo botão da caixa, antes de qualquer coisa: ele nasce desabilitado
  // na marcação porque antes do primeiro quadro esta janela não sabe que
  // permissões tem, e é aqui que o snapshot o liga ou o deixa apagado com o
  // motivo no `title`. Fora do `if` de propósito — o estado dele tem que estar
  // certo no instante em que a caixa aparecer, e não no quadro seguinte.
  atualizarPortaDoAlerta(snapshot);
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
  if (linhaAberta === null) return;

  // Com arquivo escolhido, o caminho é outro: o corpo viaja **junto** com o
  // arquivo, num fluxo só deles, e a mensagem só aparece na Linha quando os
  // bytes chegam inteiros. Um `send_message` separado publicaria o texto
  // primeiro e deixaria a foto aparecendo minutos depois, sem nada dizendo que
  // as duas coisas eram uma.
  if (anexoPendente) {
    campo.value = "";
    await subirAnexo(corpo);
    return;
  }
  if (!corpo) return;

  // Limpa antes de esperar a resposta: um campo que só esvazia depois do ida e
  // volta parece travado numa rede ruim, que é justo quando não pode parecer.
  campo.value = "";
  try {
    await invoke("send_message", { channel: linhaAberta, body: corpo });
  } catch (falha) {
    campo.value = corpo;
    console.warn("send_message:", falha);
  }
}

// ------------------------------------------------------------------ anexos
//
// ADR 0027. Três coisas moram aqui e nenhuma quarta: escolher um arquivo,
// vê-lo subir, e salvar um que chegou. **Não há abrir.** Nenhum cliente do
// SEELE abre arquivo, e é o único ponto deste desenho em que dá para ser
// estrito — então ele é estrito.

/** O arquivo escolhido e ainda não mandado, ou `null`. */
let anexoPendente = null;

/** A chave da mensagem cuja subida está em andamento, ou `null`. */
let subindo = null;

/** Onde os arquivos salvos vão, escrito por extenso antes de qualquer botão. */
let pastaDeDestino = "";

/**
 * O limite e os tipos que valem uma prévia, vindos do Rust. `null` até chegar.
 *
 * Enquanto for `null` nenhum botão de prévia é oferecido, e isso é o certo:
 * oferecer antes de saber o limite seria oferecer a partir de um palpite.
 */
let regrasDePrevia = null;

/**
 * As prévias já buscadas, por identificador de anexo.
 *
 * Existe porque a lista de mensagens é reconstruída inteira a cada
 * atualização. Sem o mapa, rolar a conversa apagaria toda figura já desenhada —
 * e redesenhá-la custaria o download de novo, do disco de quem hospeda.
 *
 * Recusa também entra aqui. Um arquivo cujos bytes discordaram do nome não
 * discorda menos na segunda tentativa.
 */
const previas = new Map();

/**
 * Guarda um arquivo para ir junto da próxima mensagem.
 *
 * O tamanho vem junto porque é ele que faz a barra ser barra: o total aqui é
 * **sempre** conhecido — quem escolheu o arquivo sabe o tamanho dele —, então
 * esta tela nunca mostra um travessão no lugar de um andamento. É a metade da
 * regra do ADR 0026 que este caminho nunca precisa da outra.
 */
async function escolherAnexo(caminho) {
  let arquivo;
  try {
    arquivo = await invoke("descrever_arquivo", { caminho });
  } catch (falha) {
    console.warn("descrever_arquivo:", falha);
    recusarAnexo("NÃO CONSEGUI LER ESSE ARQUIVO");
    return;
  }
  guardarAnexo(arquivo);
}

/**
 * Abre o seletor de arquivos do sistema.
 *
 * O botão ARQUIVO chama isto, e antes ele não chamava nada: o ADR 0027 tinha
 * decidido que escolher era arrastar, e o botão só dizia a instrução em voz
 * alta. A emenda do ADR conta o que aconteceu com essa decisão na primeira
 * pessoa que a usou.
 *
 * Desistir do seletor devolve `null` e não escreve nada em lugar nenhum: fechar
 * um seletor é o caso mais comum de abrir um seletor, e não é falha de coisa
 * nenhuma.
 */
async function abrirSeletorDeArquivo() {
  // Sem Linha aberta não há para onde mandar, e `enviar` desiste calado. Um
  // seletor que abre, escolhe, e deixa o arquivo parado sem explicação é o
  // mesmo silêncio que trouxe este arquivo aqui — então a recusa vem antes do
  // diálogo, e vem onde dá para ver.
  if (linhaAberta === null) {
    recusarAnexo("ABRA UM CANAL ANTES DE ESCOLHER UM ARQUIVO");
    return;
  }
  let arquivo;
  try {
    arquivo = await invoke("escolher_arquivo");
  } catch (falha) {
    console.warn("escolher_arquivo:", falha);
    recusarAnexo("NÃO CONSEGUI LER ESSE ARQUIVO");
    return;
  }
  if (arquivo) guardarAnexo(arquivo);
}

/** Põe na tela o arquivo que vai junto da próxima mensagem. */
function guardarAnexo(arquivo) {
  anexoPendente = arquivo;
  $("anexo-nome").textContent = arquivo.nome;
  $("anexo-tamanho").textContent = emBytes(arquivo.tamanho);
  $("anexo-barra").hidden = true;
  $("anexo-estado").textContent = "";
  $("anexo-pendente").hidden = false;
  $("campo-mensagem").focus();
}

/**
 * Diz, **onde dá para ver**, que o arquivo não entrou.
 *
 * `anunciar()` sozinho não servia aqui, e isto foi descoberto do pior jeito: a
 * `.anuncio` é uma região de um pixel recortada fora da tela, para leitor de
 * tela, e quem enxerga não vê nada. Uma falha anunciada só ali é uma falha que,
 * para quem estava olhando, não aconteceu — que é exatamente a frase do relato
 * que trouxe este arquivo.
 *
 * A caixa do anexo já é `aria-live`, então escrever nela diz as duas coisas de
 * uma vez, e um segundo `anunciar()` leria a frase duas vezes.
 */
function recusarAnexo(frase) {
  anexoPendente = null;
  $("anexo-nome").textContent = "";
  $("anexo-tamanho").textContent = "";
  $("anexo-barra").hidden = true;
  $("anexo-estado").textContent = frase;
  $("anexo-pendente").hidden = false;
}

/** Desiste do arquivo escolhido. Nada foi mandado, então nada é desfeito. */
function tirarAnexo() {
  anexoPendente = null;
  subindo = null;
  $("anexo-pendente").hidden = true;
  $("anexo-barra").hidden = true;
  $("anexo-estado").textContent = "";
}

/**
 * Manda o arquivo escolhido, com o que estiver escrito no campo.
 *
 * A chave de idempotência volta na hora e é guardada aqui: é por ela que o
 * andamento que chega encontra esta subida, e é ela que torna uma retentativa
 * segura em vez de uma segunda mensagem.
 */
async function subirAnexo(corpo) {
  const arquivo = anexoPendente;
  if (!arquivo) return;
  $("anexo-barra").hidden = false;
  $("anexo-barra").value = 0;
  $("anexo-estado").textContent = "SUBINDO";
  try {
    subindo = await invoke("enviar_anexo", {
      channel: linhaAberta,
      body: corpo,
      caminho: arquivo.caminho,
      nome: arquivo.nome,
      tipo: arquivo.tipo,
    });
  } catch (falha) {
    console.warn("enviar_anexo:", falha);
    $("anexo-estado").textContent = TRANSFERENCIAS.Fell;
  }
}

/**
 * Redesenha o andamento de uma transferência.
 *
 * **A queda tem frase própria e ela diz que recomeça do zero.** O ADR 0027 não
 * tem retomada, e uma barra que simplesmente voltasse ao começo deixaria isso
 * para a pessoa descobrir — que é a diferença entre um produto que avisa e um
 * que surpreende.
 */
function transferenciaAndou(transfer) {
  if (!transfer || typeof transfer !== "object") return;
  const tipo = transfer.kind;

  if (tipo === "Sending") {
    if (transfer.client_message_id !== subindo) return;
    $("anexo-barra").hidden = false;
    $("anexo-barra").max = transfer.total || 1;
    $("anexo-barra").value = transfer.done;
    const porcento = transfer.total
      ? Math.floor((transfer.done * 100) / transfer.total)
      : 0;
    $("anexo-estado").textContent = `SUBINDO ${porcento}%`;
    return;
  }

  if (tipo === "Sent") {
    if (transfer.client_message_id !== subindo) return;
    // O arquivo saiu inteiro. A mensagem aparece na Linha em seguida, pelo
    // caminho de sempre — o servidor publica quando os bytes chegam, e é o
    // `MessagesChanged` que a desenha.
    tirarAnexo();
    anunciar(TRANSFERENCIAS.Sent);
    return;
  }

  if (tipo === "Refused" || tipo === "Fell") {
    if (transfer.client_message_id !== subindo) return;
    $("anexo-barra").hidden = true;
    // `Refused` deixa a frase para o aviso que vem pelo controle, que é o que
    // sabe **por que**; `Fell` não tem aviso nenhum a caminho, então a frase é
    // aqui ou em lugar nenhum.
    $("anexo-estado").textContent =
      tipo === "Fell" ? TRANSFERENCIAS.Fell : TRANSFERENCIAS.Refused;
    anunciar($("anexo-estado").textContent);
    subindo = null;
    return;
  }

  // A razão, que veio pelo controle. Chega depois do `Refused` acima, e é essa
  // ordem que faz a tela dizer «recusado» na hora e **por que** um instante
  // depois, em vez de ficar calada até a segunda metade chegar.
  if (tipo === "RefusedBecause") {
    if (transfer.client_message_id !== subindo) return;
    $("anexo-barra").hidden = true;
    $("anexo-estado").textContent = fraseDeAnexo(transfer.reason);
    anunciar($("anexo-estado").textContent);
    subindo = null;
    return;
  }

  // Um arquivo pedido que não vem. O motivo esperado é `Expired`, e é assim que
  // «este arquivo expirou» chega a uma tela que já estava desenhada quando os
  // bytes saíram do servidor.
  if (tipo === "Unavailable") {
    const alvo = document.querySelector(
      `[data-anexo-estado="${transfer.attachment}"]`,
    );
    const frase = fraseDeAnexo(transfer.reason);
    if (alvo) alvo.textContent = frase;
    anunciar(frase);
    return;
  }

  if (tipo === "Receiving") {
    const alvo = document.querySelector(
      `[data-anexo-estado="${transfer.attachment}"]`,
    );
    if (!alvo) return;
    const porcento = transfer.total
      ? Math.floor((transfer.done * 100) / transfer.total)
      : 0;
    alvo.textContent = `SALVANDO ${porcento}%`;
    return;
  }

  if (tipo === "Saved" || tipo === "NotSaved") {
    const alvo = document.querySelector(
      `[data-anexo-estado="${transfer.attachment}"]`,
    );
    const frase =
      tipo === "Saved"
        ? `${TRANSFERENCIAS.Saved} — ${transfer.path}`
        : TRANSFERENCIAS.NotSaved;
    if (alvo) alvo.textContent = frase;
    anunciar(frase);
  }
}

/**
 * O bloco que uma mensagem com arquivo ganha embaixo do corpo.
 *
 * Nome, tamanho, e o que dá para fazer com ele. A prévia entra por um botão e
 * **nunca por rolar**: o arquivo mora no servidor, então ver é baixar, e uma Linha
 * que buscasse toda imagem enquanto a conversa rola transformaria o teto de
 * disco de quem hospeda em banda de todo mundo — um giga de saída cada vez que
 * alguém abrisse a Linha. Quem quer ver pede; quem só está lendo não paga.
 *
 * O botão só é oferecido quando o tipo alegado é um dos que esta janela
 * desenha e o arquivo cabe no limite da prévia. Isso é conveniência e não é a
 * regra: a regra é aplicada onde os bytes estão, e um pedido que passasse
 * daqui receberia uma recusa em vez de uma figura. Quem decide o que desenhar
 * são os primeiros bytes, e o tipo alegado é texto que a outra pessoa escolheu.
 *
 * Uma prévia já buscada é redesenhada do mapa e não é buscada de novo: esta
 * lista é reconstruída inteira a cada atualização, e sem o mapa rolar a
 * conversa custaria o download outra vez.
 *
 * E **não há botão de abrir**, em nenhum ramo, e prever não é abrir: o arquivo
 * não toca o disco, não ganha caminho, e nada fora desta janela é acionado.
 * Salvar continua sendo o único verbo com destino, e o que a pessoa faz com o
 * arquivo depois é com ela e com o sistema dela.
 */
function blocoDeAnexo(anexo) {
  const bloco = elemento("span", "anexo");
  const nome = elemento("span", "anexo-arquivo", anexo.file_name);
  bloco.append(glifo("anexo", ""), nome);
  bloco.append(elemento("span", "anexo-tamanho", emBytes(anexo.byte_size)));

  const estado = elemento("span", "anexo-estado");
  estado.dataset.anexoEstado = String(anexo.id);
  if (anexo.expired) {
    // O ADR guarda a linha depois de apagar os bytes exatamente para esta
    // frase existir. Sem ela, uma mensagem que teve foto viraria uma mensagem
    // com nada, e ninguém saberia que houve uma.
    estado.textContent = "ESTE ARQUIVO EXPIROU";
    bloco.classList.add("anexo-expirado");
  } else {
    const salvar = elemento("button", "anexo-salvar", "SALVAR");
    salvar.type = "button";
    salvar.dataset.anexoSalvar = String(anexo.id);
    salvar.dataset.anexoNome = anexo.file_name;
    salvar.title = pastaDeDestino
      ? `salvar em ${pastaDeDestino}`
      : "salvar em disco";
    bloco.append(salvar);

    const buscada = previas.get(anexo.id);
    if (buscada) {
      bloco.append(desenhoDaPrevia(buscada, anexo.file_name));
    } else if (podeOferecerPrevia(anexo)) {
      const ver = elemento("button", "anexo-previa", "PRÉVIA");
      ver.type = "button";
      ver.dataset.anexoPrevia = String(anexo.id);
      ver.title = "baixa o arquivo e desenha, se os bytes forem de imagem";
      bloco.append(ver);
    }
  }
  bloco.append(estado);
  return bloco;
}

/**
 * Se vale oferecer o botão para este anexo.
 *
 * As duas metades vêm do Rust, por `regras_de_previa`, e não estão escritas
 * aqui: uma segunda cópia da lista de tipos discordaria da primeira algum dia,
 * e discordaria oferecendo desenhar o que a busca depois recusa.
 *
 * O tipo alegado decide apenas se **há oferta**. Ele nunca decide o que
 * desenhar — isso é dos bytes, e é do outro lado da ponte.
 */
function podeOferecerPrevia(anexo) {
  if (anexo.expired || regrasDePrevia === null) return false;
  const alegado = String(anexo.declared_type || "").trim().toLowerCase();
  return (
    regrasDePrevia.types.includes(alegado) && anexo.byte_size <= regrasDePrevia.limit
  );
}

/**
 * A figura, ou a frase que explica por que não há figura.
 *
 * `previa.image` é um `data:` inteiro montado no Rust, tipo de mídia incluído,
 * a partir do que foi achado nos bytes. A página não junta tipo com bytes, e é
 * de propósito: uma página que juntasse poderia juntar com a alegação de quem
 * mandou, que é justamente o que todo este caminho recusa.
 *
 * `data:` e não URL porque a política de segurança de conteúdo desta janela é
 * `default-src 'self'` e **não afrouxa**. Ela já permite `data:` em imagem, e
 * nenhuma figura vale uma entrada nova nela.
 */
function desenhoDaPrevia(previa, nomeDoArquivo) {
  if (previa.image) {
    const figura = elemento("span", "anexo-desenho");
    const imagem = document.createElement("img");
    imagem.src = previa.image;
    // O nome do arquivo é a única descrição honesta que existe aqui: ninguém
    // deste lado sabe o que a figura mostra, e inventar uma legenda seria pior
    // do que repetir o nome que já está doze pixels acima.
    imagem.alt = nomeDoArquivo;
    figura.append(imagem);
    return figura;
  }
  return elemento("span", "anexo-recusa", fraseDePrevia(previa));
}

/**
 * Busca os bytes de um anexo e desenha o que eles disserem que ele é.
 *
 * Uma vez por anexo e por pressão de botão. O que volta é guardado no mapa,
 * inclusive quando é recusa: repetir a busca de um arquivo cujos bytes já
 * discordaram do nome gastaria a banda de quem hospeda para chegar à mesma
 * conclusão.
 *
 * A frase da recusa é escrita **no bloco**, que está na tela, e não só no
 * anúncio para leitor de tela: o ADR 0027 já pagou uma vez por uma falha
 * contada só numa caixa de um pixel, que para quem está olhando é o mesmo que
 * coisa nenhuma acontecer.
 */
async function verPrevia(anexo) {
  const botao = document.querySelector(`button[data-anexo-previa="${anexo}"]`);
  if (botao) {
    botao.disabled = true;
    botao.textContent = "BAIXANDO";
  }
  let previa;
  try {
    previa = await invoke("prever_anexo", { anexo });
  } catch (falha) {
    console.warn("prever_anexo:", falha);
    previa = { attachment: anexo, image: null, claimed: "", found: null,
               refusal: { kind: "DidNotArrive" } };
  }
  previas.set(anexo, previa);
  const nomeDoArquivo = botao?.closest(".anexo")?.querySelector(".anexo-arquivo")
    ?.textContent ?? "";
  const desenho = desenhoDaPrevia(previa, nomeDoArquivo);
  if (botao) botao.replaceWith(desenho);
  if (!previa.image) anunciar(fraseDePrevia(previa));
}

/**
 * Salva um anexo, com a consequência dita antes do ato.
 *
 * A frase de consequência é onde esta janela cumpre a parte do ADR 0027 que
 * ninguém pode descobrir depois: **quem hospeda o servidor pôde ler este
 * arquivo**, e o SEELE não varre vírus. As duas coisas são verdade e as duas
 * têm de estar escritas antes de a pessoa apertar, não numa página de ajuda.
 *
 * ---- por que `abrirConfirmacao` e não `armarAto` ----
 *
 * Porque `armarAto` **não abre a caixa**. Ele escreve a frase dentro de
 * `#moderar`, troca o corpo dela pela confirmação e põe o foco no CANCELAR — e
 * mais nada. Quem revela `#moderar` são as três portas de `camada-moderar.js`
 * (`abrirModeracao`, `abrirConfirmacao`, `abrirRecusa`), e esta função chamava o
 * miolo direto, por fora das três.
 *
 * O resultado era o defeito relatado: apertar SALVAR não fazia absolutamente
 * nada. Nenhum erro no console, nenhuma linha vermelha, nenhum quadro piscando
 * — a caixa continuava com `hidden`, o `focus()` num elemento escondido não faz
 * nada e não avisa, e o ato ficava armado numa caixa que ninguém veria. O botão
 * ficou assim desde que existe.
 *
 * Chamar a porta e não o miolo também é o que devolve o resto do acordo:
 * `focoAntesDeModerar` guarda quem apertou SALVAR, e `caixaComEscolha = false`
 * é o que faz CANCELAR fechar a caixa em vez de voltar para uma lista de
 * pessoas que este caminho nunca mostrou.
 *
 * ---- sem pasta de destino não há pergunta a fazer ----
 *
 * `pastaDeDestino` vem de `pasta_de_downloads`, lido uma vez no carregamento, e
 * o Rust devolve `""` quando a máquina não sabe dizer nem downloads nem home.
 * Com a cadeia vazia o destino virava o nome do arquivo sozinho — um caminho
 * relativo, que grava onde quer que o processo tenha sido iniciado. É o pior
 * desfecho possível: o arquivo é gravado de verdade, a frase promete um lugar
 * que não é aquele, e ninguém acha o arquivo depois. Então esta é a única
 * recusa deste caminho, e ela é dita em voz alta.
 */
function salvarAnexo(anexo, nome) {
  if (pastaDeDestino === "") {
    abrirRecusa(
      "SALVAR EM DISCO",
      "Esta máquina não disse onde ficam os arquivos baixados, e sem isso o " +
        "SEELE só saberia gravar «" +
        nome +
        "» num lugar que ele não consegue nomear para você — e um arquivo que " +
        "ninguém sabe onde foi parar é um arquivo perdido.\n" +
        "Nada foi gravado.",
    );
    return;
  }

  const destino = `${pastaDeDestino}/${nome}`;
  abrirConfirmacao(
    "SALVAR EM DISCO",
    `Grava «${nome}» em ${destino}.\n` +
      "O SEELE não confere se este arquivo é seguro: ele não varre vírus e não " +
      "vai varrer. Ele confere só que o arquivo chegou inteiro. O arquivo é " +
      "marcado com a quarentena do sistema ao ser gravado, e é o seu sistema " +
      "operacional que vai parar você na frente dele se for o caso.\n" +
      "Nenhuma tela do SEELE abre este arquivo. Abrir é um ato seu, fora daqui.",
    "SALVAR EM DISCO",
    () => invoke("salvar_anexo", { anexo, destino }),
  );
}

/**
 * Entrar num VoiceRoom, sair dele, ou abrir uma Linha.
 *
 * O alvo é um `<button>` com `data-voice_room` ou `data-linha`, e não mais a linha
 * inteira: no v2 o ouvinte estava no `<ul>` e o que se apertava era um `<li>`,
 * que nenhum teclado alcança e nenhum leitor de tela anuncia como apertável.
 */
async function alternarCanal(evento) {
  const item = evento.target.closest("button[data-voice_room], button[data-linha]");
  if (!item) return;
  let entrou = false;
  try {
    if (item.dataset.voice_room) {
      const voice_room = Number(item.dataset.voice_room);
      if (item.dataset.dentro === "sim") {
        await invoke("eject_plug");
      } else {
        await invoke("insert_plug", { voice_room });
        entrou = true;
      }
    } else if (item.dataset.linha) {
      linhaAberta = Number(item.dataset.linha);
      await invoke("open_channel", { channel: linhaAberta });
    }
    // Escolhido o destino, a gaveta fecha: ela é navegação, e a conversa que
    // se acabou de abrir está atrás dela. Em janela larga não há gaveta e isto
    // não faz nada.
    alternarCanais(false);
    // Soltos **antes** do redesenho, não depois: ver `soltarCasamentos`.
    soltarCasamentos();
    await atualizar();
    // A lista de mensagens acabou de ser trocada inteira. Ver `refazerBusca`.
    await refazerBusca();
    // Entrou numa sala de voz: a chamada abre.
    //
    // Uma sala de voz é onde se fala, e depois de entrar não há nada a fazer na
    // operação — quem está lá quer ver quem está junto, o microfone e a tela.
    // Ficar na lista de canais depois de escolher um é a tela pedindo de novo
    // uma decisão que acabou de ser tomada, e é o mesmo gesto que a caixa de
    // compartilhar já faz quando a transmissão começa.
    //
    // Só ao **entrar**. Sair de uma sala de voz devolve à operação, que é para onde
    // quem saiu está indo.
    if (entrou) await abrirChamada();
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

/**
 * O nome do servidor em que se está, para uma frase que o cite.
 *
 * O nome anunciado primeiro, o endereço discado depois. Nunca `—`: um travessão
 * dentro de uma frase de consequência é a frase deixando de dizer de quem ela
 * fala, e é justamente a metade que faz a pessoa lê-la.
 */
function nomeDesteServidor() {
  return desenhado?.server || alvoDoServer || "este servidor";
}

/**
 * Se é esta janela que está hospedando o servidor desta sessão.
 *
 * Pergunta a `estado_da_porta`, que já responde isso à camada da portaria, em
 * vez de um comando novo com a mesma resposta: quando não se hospeda ele
 * devolve `hospedando: false` sem tocar no banco do servidor, que é o caso
 * barato e o caso comum.
 *
 * Falha vira `false`, e isso não é otimismo: o `false` só **omite** uma linha da
 * consequência — a de que o servidor cai junto —, e nenhuma frase daqui afirma
 * que nada mais cai. Omitir o que não se sabe é honesto; afirmá-lo não seria.
 */
async function hospedandoAqui() {
  try {
    return (await invoke("estado_da_porta")).hospedando === true;
  } catch (falha) {
    console.warn("estado_da_porta:", falha);
    return false;
  }
}

/**
 * O que trocar de servidor custa, com os dois nomes dentro.
 *
 * A forma é a das frases da moderação, e pela mesma razão: «tem certeza?» não
 * acrescenta nada a quem já apertou uma vez, e dizer o que vai acontecer sim.
 *
 * A terceira linha só aparece para quem hospeda, e é a que muda a decisão: para
 * essa pessoa trocar de servidor não é sair de uma conversa, é fechar a
 * conversa de todo mundo. Ela não pode ser um `title` nem uma nota — é a
 * consequência, e vai onde as outras vão.
 */
function consequenciaDeTrocar(daqui, ate, hospedando) {
  const comum =
    `Você sai de ${daqui} agora, no meio do que estiver fazendo, e entra em ${ate}.\n` +
    "Dá para estar em um servidor por vez: entrar em outro é sair deste.";
  if (!hospedando) return comum;
  return (
    `${comum}\n${daqui} está no ar dentro deste computador: ele cai junto, e ` +
    "todo mundo que estiver nele sai."
  );
}

/** O que ir à entrada custa. Mesma forma, e um destino que não é servidor nenhum. */
function consequenciaDeIrParaAEntrada(daqui, hospedando) {
  const comum =
    `Você sai de ${daqui} agora, no meio do que estiver fazendo, e volta à tela ` +
    "de entrada sem estar em servidor nenhum.\n" +
    "O campo do endereço fica vazio, esperando o do servidor novo.";
  if (!hospedando) return comum;
  return (
    `${comum}\n${daqui} está no ar dentro deste computador: ele cai junto, e ` +
    "todo mundo que estiver nele sai."
  );
}

/**
 * Pergunta antes de trocar de servidor, e a pergunta diz o preço.
 *
 * **Por que perguntar aqui e não no `DESCONECTAR`**, que derruba a mesma
 * sessão: o `DESCONECTAR` é o que ele diz, e uma caixa repetindo o rótulo do
 * botão é a caixa que treina a apertar duas vezes sem ler. Estes dois controles
 * dizem outra coisa — «vá para aquele servidor», «conecte a um novo» —, e a
 * parte que não está no rótulo deles é o que a caixa existe para escrever: você
 * sai deste, e o servidor que este computador hospeda cai junto.
 *
 * A caixa é a de `camada-moderar.js`, e não uma nova. Ela não é da moderação —
 * salvar um anexo já entra por ela —, é a superfície de confirmação deste
 * produto: a que escreve a consequência com o nome dentro e põe o foco no
 * CANCELAR. Uma segunda seria uma segunda forma de esquecer de escrever a frase.
 */
async function pedirTrocaDeServidor(alvo, apelido) {
  const daqui = nomeDesteServidor();
  const hospedando = await hospedandoAqui();
  abrirConfirmacao(
    "TROCAR DE SERVIDOR",
    consequenciaDeTrocar(daqui, alvo, hospedando),
    `ENTRAR EM ${alvo}`,
    () => trocarDeServidor(alvo, apelido),
  );
}

/** O mesmo, para o `+`: a entrada, sem destino escolhido. */
async function pedirAEntrada() {
  const daqui = nomeDesteServidor();
  const hospedando = await hospedandoAqui();
  abrirConfirmacao(
    "CONECTAR A OUTRO SERVIDOR",
    consequenciaDeIrParaAEntrada(daqui, hospedando),
    `SAIR DE ${daqui}`,
    sairParaAEntrada,
  );
}

/**
 * Troca de servidor: sai deste e entra naquele.
 *
 * Desconectar-e-conectar, nesta ordem, porque é o que o produto tem: `Session`
 * guarda **um** `Plug` e `connect` devolve `AlreadyConnected` enquanto houver
 * um. Não há troca atômica a inventar aqui — a troca *é* isto.
 *
 * E ela passa pela tela de entrada de propósito, em vez de conectar por baixo
 * com a sessão ainda na frente. O caminho da entrada já sabe fazer tudo o que
 * pode dar errado: a etapa da chegada aparece enquanto ela acontece, uma batida
 * que fica pendente na portaria tem tela própria, e a recusa vira a linha
 * vermelha de `#boot-erro`. Conectando por trás da sessão, cada uma dessas
 * coisas seria escrita numa tela escondida — e quem apertasse um servidor que
 * não responde ficaria olhando o servidor anterior, que já caiu.
 *
 * O apelido é o do histórico, e não o desta sessão: é com aquele nome que esta
 * pessoa entrou naquele servidor da última vez.
 */
async function trocarDeServidor(alvo, apelido) {
  await ejetar();
  // **O convite do servidor anterior fica para trás**, e sem esta linha a troca
  // simplesmente não funcionava.
  //
  // `limparConvite` já existia, e o comentário dela já descrevia este defeito
  // com todas as letras: «o token vale para o servidor daquele link; deixá-lo
  // para trás numa troca de endereço manda a credencial de um servidor para
  // outro, que a recusa». Ela só era chamada quando alguém esvaziava o campo à
  // mão — e quem entra por link nunca esvazia.
  //
  // O sintoma é cruel de depurar porque a recusa fala da coisa errada:
  // «credencial recusada» num servidor que não pediu credencial nenhuma.
  limparConvite();
  $("campo-servidor").value = alvo;
  $("campo-apelido").value = apelido;
  await conectar();
}

/**
 * Sai para a entrada com o campo do endereço vazio.
 *
 * A diferença inteira entre o `+` e o `DESCONECTAR`, e ela é do formulário: quem
 * desconecta costuma voltar ao mesmo lugar e o endereço de lá continua no campo;
 * quem apertou «conectar a outro servidor» já disse que não é aquele, e um
 * endereço velho num formulário que promete um servidor novo é o começo de uma
 * conexão errada.
 *
 * O cursor já está no campo: `ejetar` termina em `abrirTela("tela-boot")`, e o
 * `data-foco` daquela tela é este campo.
 */
async function sairParaAEntrada() {
  await ejetar();
  // Pelo mesmo motivo de `trocarDeServidor`, e aqui é ainda mais claro: quem
  // apertou «conectar a outro servidor» já disse que não é aquele, e o token
  // daquele não tem o que fazer no formulário do próximo.
  limparConvite();
  $("campo-servidor").value = "";
}

/** Ejeta e volta para a tela de entrada, sem fechar o programa. */
async function ejetar() {
  await invoke("disconnect");
  $("tela-sessao").hidden = true;
  $("tela-fim").hidden = true;
  $("tela-boot").hidden = false;
  $("convite").hidden = true;
  $("bateria").hidden = true;
  // A moderação pela mesma razão que a bateria: aberta, ela voltaria por cima
  // da próxima sessão com um ato armado sobre alguém do servidor anterior.
  abandonarModeracao();
  // O veredito era sobre a chave daquela sessão. Deixá-lo aceso sobre a
  // próxima seria dizer de um servidor o que se apurou de outro.
  mostrarVeredito(null);
  document.body.classList.remove("na-bateria");
  // A gaveta pela mesma razão que a moderação: aberta, ela voltaria por cima
  // da próxima sessão com a lista de salas da anterior ainda desenhada.
  alternarCanais(false);
  desenhado = null;
  linhaAberta = null;
  // O mesmo argumento do veredito, para o relógio e para o endereço: um uptime
  // que sobrevive à sessão conta o tempo passado noutro servidor, e um endereço
  // que sobrevive põe no cabeçalho do próximo a porta do anterior.
  comecoDaSessao = null;
  alvoDoServer = null;
  // A conversa e a revisão pela mesma razão. A revisão do próximo servidor começa
  // em zero, e guardar a do anterior faria a primeira sincronização concluir
  // que nada mudou — a tela abriria com o histórico de outra sessão na frente.
  mensagens = [];
  revisaoDesenhada = null;
  // Fecha a barra, e não só zera o termo: aberta, ela abriria a próxima sessão
  // já com o cursor num campo de busca sobre uma conversa que não existe.
  await alternarBusca(false);
  // O convite não sobrevive à sessão que ele abriu: quem sai, digita outro
  // endereço e aperta INSERT mandaria o token do servidor anterior ao novo.
  limparConvite();
  // E os três blocos do boot voltam a apagar. Deixá-los acesos seria a tela de
  // entrada afirmando uma conexão que acabou de ser desfeita.
  subsistemas("", "·");
  // Quem acabou de sair de um servidor tem que vê-lo na lista.
  await desenharVisitados();
  // E o teclado sai junto. Sem isto o foco fica no `<body>`: quem apertou
  // DESCONECTAR com a tecla volta para a entrada tendo de tabular a tela toda
  // até o campo de endereço, que é a única coisa que se faz nela.
  abrirTela("tela-boot");
}

/** Zera o campo, o cursor no Rust e o realce. */
async function encerrarBusca() {
  $("campo-busca").value = "";
  await invoke("busca_limpar");
  limparBusca();
}

/**
 * Abre ou fecha a barra de busca.
 *
 * Ela ficava aberta o tempo todo, gastando 40px da coluna em toda sessão para
 * uma coisa que se faz uma vez por hora. O v3 põe um `BUSCAR` rotulado no
 * cabeçalho da Linha, e a tecla `/` continua valendo — as duas portas levam
 * aqui.
 *
 * Fechar encerra a busca de verdade, e não só esconde: um realce aceso atrás de
 * uma barra fechada é a tela afirmando um termo que ninguém consegue mais ler
 * nem mudar.
 */
async function alternarBusca(abrir) {
  const barra = $("form-busca");
  const querAbrir = abrir ?? barra.hidden;
  barra.hidden = !querAbrir;
  $("botao-buscar").setAttribute("aria-expanded", querAbrir ? "true" : "false");
  if (querAbrir) {
    $("campo-busca").focus();
    return;
  }
  await encerrarBusca();
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
  desenharMensagens();
  if (estado.atual) {
    // A ocorrência, e não a mensagem. Rolar até a mensagem punha na tela a
    // linha certa e nada dentro dela: numa mensagem que casa três vezes,
    // avançar duas vezes rolava para o mesmo lugar e mexia só no algarismo.
    //
    // O recurso é pelo `data-mensagem` e não mais pelo lugar entre os filhos:
    // a lista ganhou divisores de dia, e cada divisor empurraria essa conta em
    // um — caladamente, e só para as mensagens depois dele.
    const alvo =
      $("lista-mensagens").querySelector(".realce-atual") ??
      $("lista-mensagens").querySelector(`[data-mensagem="${estado.atual.message}"]`);
    alvo?.scrollIntoView({ block: "center" });
  }
}

function limparBusca() {
  casamentosPorMensagem = new Map();
  ocorrenciaAtual = null;
  $("busca-contador").textContent = "";
  desenharMensagens();
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
 * entrar ou sair de um VoiceRoom, uma mensagem editada ou apagada. Ao contrário do
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

// ------------------------------------------------ a janela estreita, e a ordem
//
// Quando a janela não cabe nas quatro colunas, alguma delas tem de sair — e a
// ordem em que saem é a decisão, não o fato de saírem.
//
// 1. A faixa de pessoas recolhe primeiro. Não é onde se trabalha, e o que ela
//    mostra não sai da tela com ela: a coluna ao lado — que só cede no ponto
//    seguinte — lista quem está em cada sala, e a tela de chamada tem a sala
//    ocupada em cartões, a um botão do cabeçalho.
// 2. A coluna de salas e canais vira gaveta depois. Ela é navegação — algo que
//    se usa para chegar a um canal e não enquanto se lê o que está nele.
// 3. A conversa nunca é a primeira a apertar. Ela é o painel em que se passam
//    horas, e o único desta tela em que a largura muda o que cabe numa linha.
//
// A folha faz (1) e (2) sozinha; o que mora aqui é a porta da gaveta, porque
// uma sobreposição que se abre sem controle nenhum é uma coluna que sumiu.

/**
 * O botão que abre a gaveta de salas e canais.
 *
 * Escrito aqui e não na marcação porque ele só existe por causa do que a folha
 * faz com a largura, e as duas metades têm de morrer juntas no dia em que a
 * gaveta sair. Ele mora na barra do canal aberto — que é a única barra que
 * continua na tela em toda largura — e a folha o esconde acima do ponto de
 * quebra: um botão que abre uma coluna já visível é um botão que não faz nada.
 */
const botaoDeCanais = elemento("button", "linha-canais", "CANAIS");
botaoDeCanais.type = "button";
botaoDeCanais.setAttribute("aria-expanded", "false");
botaoDeCanais.title = "as salas de voz e os canais de texto";
document.querySelector(".linha-barra")?.prepend(botaoDeCanais);

/**
 * Abre ou fecha a gaveta.
 *
 * Fechada, a coluna sai com `display: none` e não deslocada para fora da tela:
 * fora da tela ela continua no caminho do teclado, e tabular por trinta botões
 * invisíveis para chegar ao campo de mensagem é pior do que não ter a coluna.
 *
 * O foco acompanha, nos dois sentidos. Abrir uma sobreposição e deixar o foco
 * atrás dela é abrir para quem enxerga e para mais ninguém; fechá-la sem
 * devolver o foco larga quem usa o teclado no começo da tela.
 */
function alternarCanais(abrir) {
  const tela = $("tela-sessao");
  const aberta = tela.dataset.canais === "abertos";
  const querAbrir = abrir ?? !aberta;
  if (querAbrir === aberta) return;
  if (querAbrir) {
    tela.dataset.canais = "abertos";
    botaoDeCanais.setAttribute("aria-expanded", "true");
    $("lista-voice_rooms").querySelector("button")?.focus();
    return;
  }
  // O foco volta para o botão só quando ele estava **dentro** da gaveta, que é
  // quando fechá-la o deixaria em lugar nenhum. Devolvê-lo sempre roubaria o
  // clique de quem fechou a gaveta apontando para o campo de mensagem.
  const perdido = document
    .querySelector(".painel-canais")
    ?.contains(document.activeElement);
  delete tela.dataset.canais;
  botaoDeCanais.setAttribute("aria-expanded", "false");
  if (perdido) botaoDeCanais.focus();
}

botaoDeCanais.addEventListener("click", () => alternarCanais());

// Tocar na conversa fecha a gaveta, porque tocar na conversa é o que se faz
// depois de escolher onde falar. `pointerdown` e não `click`: com o clique, o
// primeiro toque na coluna de baixo só fechava a gaveta e o segundo é que
// chegava ao campo.
document
  .querySelector(".painel-linha")
  ?.addEventListener("pointerdown", () => alternarCanais(false));

// ------------------------------------------------------------------- ligação

$("form-busca").addEventListener("submit", (evento) => evento.preventDefault());

$("campo-busca").addEventListener("input", refazerBusca);

$("busca-proxima").addEventListener("click", async () =>
  desenharBusca(await invoke("busca_andar", { adiante: true })),
);
$("busca-anterior").addEventListener("click", async () =>
  desenharBusca(await invoke("busca_andar", { adiante: false })),
);
$("botao-buscar").addEventListener("click", () => alternarBusca());
$("busca-fechar").addEventListener("click", () => alternarBusca(false));
$("form-mensagem").addEventListener("submit", enviar);

// Arrastar é o segundo jeito de escolher um arquivo, e não é mais o único: o
// botão ARQUIVO abre o seletor do sistema. Este ouvinte continua porque quem
// arrasta espera que arrastar funcione — não porque não haja alternativa.
//
// `listen()` não é chamada de JavaScript: é chamada de IPC ao plugin `event`, e
// todo comando de plugin passa pela ACL. `capabilities/janela.json` é o que faz
// esta linha ter efeito, e sem aquele arquivo isto aqui era um ouvinte escrito
// que nunca recebia nada. `tests/permissoes.rs` guarda essa metade.
listen("tauri://drag-drop", (evento) => {
  const caminhos = evento?.payload?.paths;
  if (!Array.isArray(caminhos) || caminhos.length === 0) return;
  // Fora desta tela, calado: quem está no Terminal servidor não largou o arquivo
  // aqui. Sem Linha aberta, **não** calado — o gesto foi nesta tela, e o
  // silêncio é indistinguível de defeito.
  if ($("tela-sessao").hidden) return;
  if (linhaAberta === null) {
    recusarAnexo("ABRA UM CANAL ANTES DE ANEXAR UM ARQUIVO");
    return;
  }
  // Um por vez, e o primeiro. Uma mensagem carrega um arquivo — o ADR 0027 dá
  // ao anexo uma linha por mensagem —, e pegar cinco calado deixaria quatro
  // deles sumindo sem explicação.
  if (caminhos.length > 1) anunciar("UM ARQUIVO POR MENSAGEM; PEGUEI O PRIMEIRO");
  escolherAnexo(caminhos[0]);
});
$("anexo-tirar").addEventListener("click", tirarAnexo);
$("botao-anexar").addEventListener("click", abrirSeletorDeArquivo);
$("lista-mensagens").addEventListener("click", (evento) => {
  const salvar = evento.target.closest("button[data-anexo-salvar]");
  if (salvar) {
    salvarAnexo(Number(salvar.dataset.anexoSalvar), salvar.dataset.anexoNome);
    return;
  }
  // A prévia é buscada aqui e em nenhum outro lugar: no clique, e nunca na
  // rolagem nem no redesenho. Ver é baixar, e o teto de disco de quem hospeda
  // não pode virar banda de todo mundo por alguém abrir uma Linha.
  const ver = evento.target.closest("button[data-anexo-previa]");
  if (ver) verPrevia(Number(ver.dataset.anexoPrevia));
});
invoke("pasta_de_downloads")
  .then((pasta) => {
    pastaDeDestino = pasta;
  })
  .catch((falha) => console.warn("pasta_de_downloads:", falha));
invoke("regras_de_previa")
  .then((regras) => {
    regrasDePrevia = regras;
  })
  .catch((falha) => console.warn("regras_de_previa:", falha));
// Duas listas, um manipulador: as salas de voz e as Linhas ganharam cabeçalhos
// próprios (`B·03` e `B·04`) e deixaram de caber numa lista só.
$("lista-voice_rooms").addEventListener("click", alternarCanal);
$("lista-linhas").addEventListener("click", alternarCanal);

/**
 * Pede uma sala nova ao servidor.
 *
 * Não há resposta a esperar, e isso é do desenho e não descuido: o servidor
 * anuncia a sala a **todos** os conectados, inclusive a quem pediu, e o anúncio
 * chega pela mesma porta de sempre. A tela redesenha porque a lista mudou, e
 * não porque este botão voltou.
 *
 * Quando a permissão falta, o que volta é um aviso de `PermissionDenied`, pelo
 * mesmo caminho de qualquer outro aviso. O formulário nem devia estar visível
 * nesse caso — mas quem chegar aqui assim mesmo é recusado pelo servidor, que é
 * onde a regra vale.
 */
/// Recebe o pedido **já feito**, e não o nome do comando.
///
/// Parece indireção à toa e não é. `tests/frontend.rs` amarra cada `invoke("…")`
/// literal a um comando registrado em `main.rs`, nos dois sentidos, procurando o
/// texto — um nome de comando que chega por parâmetro some desse laço, e o
/// guarda passa a dizer que o comando vivo nunca é chamado. Foi exatamente o que
/// aconteceu na primeira versão desta função, e antes dela na tabela de
/// dispositivos do Terminal server. Duas vezes no mesmo dia: o literal fica no
/// lugar da chamada, sempre.
async function pedirSala(pedido, campo, rotulo) {
  try {
    await pedido;
    // Limpo só depois de o pedido ter saído. Limpar antes perderia o que a
    // pessoa escreveu se a chamada estourasse.
    campo.value = "";
  } catch (falha) {
    console.warn(`${rotulo}:`, falha);
  }
}

$("criar-voice_room").addEventListener("submit", (evento) => {
  evento.preventDefault();
  const nome = $("campo-voice_room-nome");
  pedirSala(
    invoke("criar_voice_room", {
      name: nome.value.trim(),
      limit: Number($("campo-voice_room-limite").value),
      channel: null,
    }),
    nome,
    "criar_voice_room",
  );
});

$("criar-linha").addEventListener("submit", (evento) => {
  evento.preventDefault();
  const nome = $("campo-linha-nome");
  pedirSala(invoke("criar_linha", { name: nome.value.trim() }), nome, "criar_linha");
});
// O `×` da caixa de alerta, e o único fechamento dela: o `RECONHECER` de baixo
// saiu, porque duas portas para o mesmo ato numa caixa de 720px é a pessoa
// procurando qual das duas é a certa. O `id` não mudou junto com a forma — é o
// mesmo elemento, noutro canto.
/**
 * Fecha o alerta, dos dois lados.
 *
 * Esconder aqui e mais nada era o bug: `desenharAviso` roda a cada redesenho,
 * duas vezes por segundo, e reabria a caixa a partir do `Snapshot`, que ainda
 * trazia o aviso. Apagar uma sala com alguém dentro cobria a janela com um
 * alerta que voltava mais rápido do que dava para clicar.
 *
 * Esconder **antes** de mandar: o comando é uma travessia de ida e volta, e a
 * caixa tem de sumir no aperto e não no quadro seguinte.
 */
function fecharAlerta() {
  $("banner").hidden = true;
  invoke("dispensar_aviso").catch((falha) => console.warn("dispensar_aviso:", falha));
}

$("banner-fechar").addEventListener("click", fecharAlerta);
// E o mesmo fechamento pela tecla. Com o `×` sozinho, `Escape` deixou de ser
// conveniência: uma caixa que cobre a janela inteira e só se fecha com o
// ponteiro é pior que a redundância que ela perdeu. Só com ela na frente, ou a
// tecla seria engolida de quem está fechando uma busca atrás dela.
window.addEventListener("keydown", (evento) => {
  if (evento.key === "Escape" && !$("banner").hidden) {
    evento.preventDefault();
    fecharAlerta();
  }
});
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
// O ouvinte fica na lista inteira e não no cartão: os cartões agora vêm dentro
// de um `<ul>` por sala, e um ouvinte por grupo seria um por sala criada.
$("lista-roster").addEventListener("input", (evento) => {
  const alvo = evento.target;
  if (!alvo.classList.contains("volume")) return;
  const percent = Number(alvo.value);
  volumes.set(alvo.dataset.pessoa, percent);
  invoke("set_volume", { nickname: alvo.dataset.pessoa, percent }).catch((falha) => {
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

$("botao-desconectar").addEventListener("click", ejetar);

// A trilha, por delegação: os botões do histórico são reconstruídos quando a
// lista muda, e um ouvinte por botão seria um ouvinte perdido a cada redesenho.
$("trilha-outros").addEventListener("click", (evento) => {
  const item = evento.target.closest("button[data-alvo]");
  if (!item) return;
  pedirTrocaDeServidor(item.dataset.alvo, item.dataset.apelido);
});

$("trilha-adicionar").addEventListener("click", pedirAEntrada);

// A barra de espaço fala, exceto enquanto se digita — a mesma colisão que a TUI
// resolve mantendo o push-to-talk fora do modo de inserção (decisão D19).
window.addEventListener("keydown", (evento) => {
  // `/` foca a busca, como no terminal. Só fora de um campo de texto — uma
  // barra digitada numa mensagem é uma barra — e só com a sessão na tela, ou
  // engoliria a tecla para focar um campo que está escondido.
  if (evento.key === "/" && !digitando() && !$("tela-sessao").hidden) {
    evento.preventDefault();
    // Abre a barra antes de focar: `focus()` num campo dentro de um `hidden`
    // não faz nada e não avisa, e a tecla passaria a não fazer nada.
    alternarBusca(true);
    return;
  }
  if (evento.target === $("campo-busca")) {
    if (evento.key === "Escape") {
      evento.preventDefault();
      alternarBusca(false);
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
  // `Escape` fecha a gaveta, depois da busca e antes da barra de espaço: com a
  // gaveta aberta sobre a conversa, ela é a coisa mais de cima que a tecla
  // pode estar querendo dispensar.
  if (evento.key === "Escape" && $("tela-sessao").dataset.canais === "abertos") {
    evento.preventDefault();
    alternarCanais(false);
    return;
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
  // O andamento de um arquivo. Não passa por `atualizar()`: enquanto sobe não
  // existe mensagem nenhuma na Linha — o servidor só publica quando os bytes
  // chegam inteiros —, então não há snapshot que carregue esta informação.
  if (payload && typeof payload === "object" && payload.TransferChanged) {
    transferenciaAndou(payload.TransferChanged.transfer);
    return;
  }

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
