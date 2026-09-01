// SEELE — a tela de chamada (`#tela-chamada`).
//
// A grade de quem está na sala de voz: um cartão por pessoa, com estado em
// palavra, sinal e volume; e o monitor da sala à direita. É a tela `chamada` do comp
// v3 (`.superpowers/sdd/comp-inventario-v3.md` §7).
//
// ---- o que este arquivo não decide ----
//
// Nada. A faixa de cada sincronia chega decidida do core (`SignalBand`), quem
// está falando chega no snapshot, a severidade de um aviso chega decidida, e as
// quatro ações são quatro `invoke`. Não há limiar, não há média calculada aqui
// e não há estado derivado de protocolo — a mesma regra que o topo de `base.js`
// escreve para a janela inteira, e que `specs/06-clientes-gui.md` escreve para
// os dois clientes.
//
// ---- o que saiu, e por quê ----
//
// O v2 desta tela desenhava metade dos campos vazios, com travessão e `title`
// explicando a falta. O v3 é a resposta a um teste entre duas máquinas que
// achou a interface fiel e difícil ao mesmo tempo, e numa tela que existe para
// ser simples um travessão explicado é ruído: quem entra numa sala de voz quer saber
// quem está falando, não quais campos este protocolo ainda não carrega.
//
// Então saíram, em vez de ficarem vazios:
//
//   - **a onda por pessoa.** `Telemetry.input_level` é escalar e é **nosso** —
//     amplitude por interlocutor não atravessa a fronteira e não é derivável de
//     nada que atravesse. Vinte e quatro barras paradas seriam vinte e quatro
//     afirmações de silêncio; oscilando, vinte e quatro mentiras animadas com o
//     nome de outra pessoa embaixo.
//   - **o atraso por pessoa.** `Telemetry.rtt_ms` é o desta máquina até o
//     servidor, um número só. Ele continua na telemetria da tela de operação, que
//     é onde ele é verdade.
//   - **a tag de subsistema** (`MEDIA·01`), que o protocolo não tem.
//   - **a rota e o cronômetro da conexão**, pela mesma razão — ver `EM SESSÃO`,
//     abaixo, para o que ficou no lugar do segundo.
//
// O registro dessas ausências não sumiu do produto: ele está no inventário
// (§1.3, §7 e §9.7), que é onde alguém procura antes de tentar desenhá-las de
// novo. Uma tela não é o lugar de guardar a lista do que o protocolo não tem.
//
// ---- o que vem de fora ----
//
// `naoMedido`, `medido`, `duracao`, `comecoDaSessao`, `blocos` e `volumes` são
// declarados em `tela-sessao.js`, e este arquivo os usa em vez de escrever os
// seus. Os scripts dividem um escopo global (ADR 0019, e o bloco de abertura de
// `base.js`), e `tela-fim.js` já faz o mesmo com `desenhado` e `linhaAberta`.
//
// `volumes` em particular **precisa** ser o mesmo mapa das duas telas: é a
// lembrança de quanto esta janela mandou para cada pessoa, e duas cópias dela
// são duas telas discordando sobre o mesmo controle. `comecoDaSessao` pela
// mesma razão — o relógio daqui e o `UPTIME` de lá contam a mesma coisa.

"use strict";

/** Quantos blocos a barra de sinal do cartão tem. */
const BLOCOS_DA_BARRA = 24;

/** Quantas células o controle de volume tem. */
const CELULAS_DE_VOLUME = 10;

/**
 * Até onde o volume por pessoa vai, e de quanto em quanto ele anda.
 *
 * `set_volume` aceita até 400 e o deslizante do roster sempre andou de 0 a 200
 * de dez em dez; estes são os mesmos números, para que o mesmo mapa `volumes`
 * signifique a mesma coisa nas duas telas. Dez células de vinte pontos põem o
 * normal exatamente na quinta.
 */
const VOLUME_MAXIMO = 200;
const VOLUME_NORMAL = 100;
const PASSO_DE_VOLUME = 10;

/**
 * Os eventos que **esta janela** viu passar, do mais novo para o mais velho.
 *
 * O comp desenha cinco linhas de histórico — `NUNES.S entrou`,
 * `HORAKI.H saiu` — e o inventário §9.8 deixou em aberto quanto disso o servidor
 * guarda. A resposta é: nada. `Event::RosterChanged` diz que o roster mudou,
 * nunca o quê, e não há histórico de entrada, saída nem mudo em lugar
 * nenhum do core.
 *
 * Então esta lista carrega só o que passou por aqui de verdade: os avisos que o
 * core levantou desde que a janela abriu, e as duas chaves que esta pessoa
 * virou nesta tela. É pouco, e é honesto — a alternativa era escrever um
 * passado que ninguém registrou.
 */
const EVENTOS_DA_CHAMADA = [];

/** Quantos eventos ficam. O painel tem altura para uns dez; o resto é rolagem. */
const EVENTOS_LEMBRADOS = 40;

// ------------------------------------------------------------------- desenho

/**
 * A tela inteira, a partir de um snapshot — ou da falta de um.
 *
 * `null` é o estado normal desta tela sem sessão, e não um erro: ela é
 * alcançável só de dentro da operação, mas a sessão pode acabar com ela aberta.
 */
function desenharChamada(snapshot) {
  const voice_room = snapshot ? snapshot.voice_rooms.find((c) => c.occupied_by_us) : null;

  desenharBarraDaChamada(snapshot, voice_room);
  desenharPalco(snapshot, voice_room);
  desenharCartoes(snapshot, voice_room);
}

/**
 * O palco: a transmissão de tela desta sala, ou o que a tela diz sem nenhuma.
 *
 * Três coisas moram aqui e nenhuma delas é decisão:
 *
 *   1. **quem** está compartilhando, que é o consentimento da §4 da spec de
 *      22/08 aparecendo em algum lugar — no Windows a captura do sistema não
 *      pergunta nada a ninguém, e esta linha é o único aviso que existe;
 *   2. **o que está saindo agora, ao lado do que foi pedido** (§5). Escolher
 *      1080p e receber 720p não é defeito; esconder que aconteceu, é;
 *   3. **a razão**, `720p · 6 pessoas assistindo` (§5.1). O gatilho da
 *      resolução é o teto e não a contagem de gente — resolução não controla
 *      tráfego —, mas quem lê quer previsibilidade, e a contagem é a metade do
 *      pedido que estava certa.
 *
 * O que **não** mora aqui é a imagem. O contrato entre a ponte e esta casca
 * carrega os números da transmissão e nenhum quadro, então esta janela não tem
 * por onde desenhar a tela de outra pessoa — e a nota do palco diz isso em vez
 * de deixar uma moldura preta parecendo uma transmissão travada.
 *
 * `nomeDaResolucao` vem de `camada-compartilhar.js`. O que foi **pedido** já não
 * vem de lá: `tela.pedido` atravessa no próprio `Snapshot`, ao lado do que está
 * saindo, porque uma memória guardada nesta janela morria com ela — e uma
 * recarga no meio de uma transmissão apagava metade da comparação do §5.
 */
function desenharPalco(snapshot, voice_room) {
  const palco = $("palco");
  const tela = snapshot ? snapshot.tela : null;

  if (!tela) {
    palco.dataset.transmitindo = "nao";
    $("palco-razao").textContent = "";
    $("palco-quem").textContent = snapshot
      ? "NINGUÉM ESTÁ COMPARTILHANDO A TELA"
      : "SEM SESSÃO";
    const nota = $("palco-nota");
    // Fora de uma sala de voz não há para quem compartilhar, e a frase que
    // manda apertar um botão desabilitado é pior que nenhuma. Sem o controle
    // desenhado, mandar apertar um botão que não existe é pior ainda.
    nota.hidden = !voice_room;
    nota.textContent = temControleDeTela()
      ? "Use COMPARTILHAR A TELA aqui embaixo para mostrar um monitor ou uma janela sua."
      : "Esta versão não sabe compartilhar tela desta máquina. A de outra pessoa apareceria aqui.";
    $("palco-parada").hidden = true;
    $("palco-numeros").hidden = true;
    $("palco-aperto").hidden = true;
    botaoDeCinema(false);
    return;
  }

  palco.dataset.transmitindo = "sim";

  // `tela.de` é o identificador de quem compartilha, e o nome vem do roster da
  // sala. Sem casamento — a pessoa saiu entre dois quadros — a frase diz que
  // alguém está, e não um nome que esta janela inventaria.
  const dono = voice_room ? voice_room.people.find((pessoa) => pessoa.id === tela.de) : null;
  $("palco-quem").textContent = tela.e_minha
    ? "VOCÊ ESTÁ COMPARTILHANDO A SUA TELA"
    : dono
      ? `${dono.nickname} ESTÁ COMPARTILHANDO A TELA`
      : "ALGUÉM DESTA SALA ESTÁ COMPARTILHANDO A TELA";

  // `720p · 6 pessoas assistindo` quando há a medida, e só a contagem quando
  // não há: `0p` é o que a ponte manda enquanto ninguém mede o que está saindo,
  // e escrevê-lo seria a mentira confiante que `TelaEmCurso::medida` existe
  // para não produzir.
  $("palco-razao").textContent = tela.medida
    ? `${nomeDaResolucao(tela.altura)} · ${assistindo(tela.espectadores)}`
    : assistindo(tela.espectadores);

  // As duas frases diziam que a imagem não aparecia aqui, e diziam a verdade
  // até a imagem passar a aparecer. A de quem transmite ainda tem o que
  // explicar: o que ela vê é o espelho do que saiu, e não a própria tela sendo
  // redesenhada — a diferença aparece quando um quadro é descartado pelo teto.
  const nota = $("palco-nota");
  nota.hidden = false;
  nota.textContent = tela.e_minha
    ? "Esta é a imagem que está saindo daqui — a mesma que a sala recebe."
    : "Duplo clique na imagem para vê-la em tela cheia.";

  // O motivo de uma parada chega como **nome** (`TelaEmCurso::parada`), e a
  // frase é escrita aqui, como a de todo enum deste produto. O `??` cobre um
  // nome que esta janela não conhece: diz que parou, que é o que ela sabe, e
  // não inventa a causa.
  const parada = $("palco-parada");
  parada.hidden = !tela.parada;
  parada.textContent = tela.parada ? (PARADAS[tela.parada] ?? "A TELA PAROU") : "";

  desenharNumerosDoPalco(tela);
  botaoDeCinema(true);
}

/**
 * O que está saindo agora, ao lado do que foi pedido.
 *
 * A coluna do pedido **some** para quem só assiste, e não vira travessão: o
 * teto é a escolha de outra pessoa e não atravessa em lugar nenhum, então não é
 * um buraco na resposta a uma pergunta que esta tela fez — é uma pergunta que
 * ela não tem por que fazer. Para quem compartilha ela fica, inclusive vazia,
 * porque aí a pergunta é legítima e a falta tem motivo escrito.
 */
function desenharNumerosDoPalco(tela) {
  $("palco-numeros").hidden = false;

  // `medida` é o que separa «está saindo a 720p» de «ninguém mediu o que está
  // saindo». Os três são `u32` e vêm zerados enquanto ninguém os mede; escrever
  // `0p` ali seria dizer que a transmissão está em zero pixel de altura.
  if (tela.medida) {
    medido($("palco-altura"), nomeDaResolucao(tela.altura));
    medido($("palco-quadros"), `${tela.quadros}/s`);
    medido($("palco-banda"), `${tela.kbps} kbps`);
  } else {
    const motivo = "ainda não há medida desta transmissão";
    naoMedido($("palco-altura"), motivo);
    naoMedido($("palco-quadros"), motivo);
    naoMedido($("palco-banda"), motivo);
  }

  for (const lado of document.querySelectorAll(".chamada-palco-pedido")) {
    lado.hidden = !tela.e_minha;
  }

  // `tela.pedido` já vem `null` para quem só assiste — o teto é escolha de quem
  // compartilha e não viaja no fio —, então não há o que ramificar aqui.
  const pedido = tela.pedido ?? null;
  if (tela.e_minha && !pedido) {
    const motivo = "esta sessão não escolheu os limites desta transmissão";
    naoMedido($("palco-altura-pedida"), motivo);
    naoMedido($("palco-quadros-pedidos"), motivo);
    naoMedido($("palco-banda-pedida"), motivo);
  } else if (pedido) {
    medido($("palco-altura-pedida"), nomeDaResolucao(pedido.altura_maxima));
    medido($("palco-quadros-pedidos"), `${pedido.quadros_maximos}/s`);
    medido(
      $("palco-banda-pedida"),
      pedido.banda_bps === null ? "sem teto seu" : `${Math.round(pedido.banda_bps / 1000)} kbps`,
    );
  }

  // E quando aperta, a tela diz que apertou e por quê (§5.1). Sem esta linha o
  // que sobra são dois números diferentes lado a lado e nenhuma explicação de
  // qual é o certo — que é o produto sabendo algo que quem está na frente dele
  // não sabe.
  const aperto = $("palco-aperto");
  const abaixo =
    tela.medida &&
    pedido !== null &&
    (tela.altura < pedido.altura_maxima ||
      tela.quadros < pedido.quadros_maximos ||
      (pedido.banda_bps !== null && tela.kbps * 1000 < pedido.banda_bps));
  aperto.hidden = !abaixo;
  aperto.textContent = abaixo
    ? "Está saindo menos do que você pediu: a conexão não dá conta de mandar " +
      "mais para todo mundo que está assistindo."
    : "";
}

/** `6 pessoas assistindo`, e as duas contagens que não pluralizam assim. */
function assistindo(quantos) {
  if (quantos === 0) return "ninguém assistindo";
  if (quantos === 1) return "1 pessoa assistindo";
  return `${quantos} pessoas assistindo`;
}

/** A barra de cima: o nome da sala, o enlace e o relógio da sessão. */
function desenharBarraDaChamada(snapshot, voice_room) {
  const nome = $("chamada-nome");
  if (voice_room) {
    medido(nome, voice_room.name);
  } else {
    naoMedido(nome, snapshot ? "fora de uma sala de voz" : "sem sessão");
  }

  const caixa = $("chamada-enlace");
  const palavra = $("chamada-conexao");
  if (!snapshot) {
    caixa.dataset.enlace = "nenhum";
    palavra.textContent = "SEM SESSÃO";
  } else {
    // `link` chega decidido: `"Online"` ou a bateria interna com os segundos
    // que restam. Quem conta os cinco minutos é a camada da bateria, sobre a
    // tela de operação; aqui só se diz em qual dos dois estados o enlace está.
    const naBateria = snapshot.link && snapshot.link !== "Online";
    caixa.dataset.enlace = naBateria ? "bateria" : "no-ar";
    palavra.textContent = naBateria ? "CONEXÃO CAIU" : "CONEXÃO NO AR";
  }

  // `duracao` no comp é o tempo de chamada. O protocolo não carrega quando a
  // pessoa entrou na sala, e um cronômetro que começasse a contar ao abrir esta
  // tela diria que ela entrou agora — que é pior que não dizer nada. O que existe é
  // o relógio local desde o primeiro quadro da sessão, e é o mesmo valor que a
  // tela de operação mostra como `UPTIME`: lido do mesmo lugar, os dois nunca
  // discordam. O rótulo na marcação diz `EM SESSÃO` justamente porque é isto
  // que ele está contando.
  const tempo = $("chamada-duracao");
  if (comecoDaSessao === null) {
    naoMedido(tempo, "esta janela ainda não desenhou um quadro desta sessão");
  } else {
    medido(tempo, duracao(comecoDaSessao));
  }
}

/**
 * Um cartão por pessoa, ou a frase do vazio.
 *
 * Os cartões são **repintados**, e não refeitos, quando já existem. Não é
 * economia: esta tela redesenha duas vezes por segundo e o cartão agora tem
 * botões dentro. Refazer os nós a cada volta tiraria o foco de quem navega por
 * teclado a cada meio segundo, e um clique que atravessasse a troca sumiria sem
 * erro nenhum — o `mouseup` cairia num nó que não é mais o do `mousedown`.
 */
function desenharCartoes(snapshot, voice_room) {
  const grade = $("chamada-grade");

  if (!voice_room || voice_room.people.length === 0) {
    const frase = !snapshot
      ? "SEM SESSÃO"
      : voice_room
        ? "ESTA SALA ESTÁ VAZIA"
        : "VOCÊ NÃO ESTÁ EM NENHUMA SALA";
    if (grade.children.length === 1 && grade.children[0].textContent === frase) return;
    repovoar(grade, [elemento("li", "chamada-vazio", frase)]);
    return;
  }

  const existentes = new Map();
  for (const cartao of grade.children) {
    if (cartao.dataset.pessoa) existentes.set(cartao.dataset.pessoa, cartao);
  }

  const cartoes = voice_room.people.map((pessoa) => {
    const cartao = existentes.get(pessoa.nickname) ?? cartaoDoPersono(pessoa);
    pintarCartao(cartao, pessoa, snapshot.audio_available);
    return cartao;
  });

  const mesma =
    cartoes.length === grade.children.length &&
    cartoes.every((cartao, onde) => cartao === grade.children[onde]);
  if (!mesma) repovoar(grade, cartoes);
}

/**
 * O esqueleto de um cartão, sem valor nenhum dentro.
 *
 * Só a forma mora aqui; o que ela mostra é `pintarCartao`. A divisão é o que
 * permite repintar sem refazer, e é ela que mantém o foco e o clique de pé
 * entre dois quadros.
 */
function cartaoDoPersono(pessoa) {
  const cartao = elemento("li", "chamada-cartao");
  cartao.dataset.pessoa = pessoa.nickname;

  // ---- topo: iniciais, nome, o estado em frase, o estado em palavra ----
  const topo = elemento("div", "chamada-cartao-topo");
  const identidade = elemento("div", "chamada-cartao-identidade");

  // As iniciais são decoração: elas repetem o nome que está do lado, em forma.
  // Anunciá-las seria ler o nome duas vezes.
  const avatar = elemento("span", "chamada-avatar");
  avatar.setAttribute("aria-hidden", "true");

  const quem = elemento("div", "chamada-cartao-quem");
  quem.append(elemento("span", "chamada-cartao-nome"), elemento("span", "nota"));
  identidade.append(avatar, quem);

  const estados = elemento("div", "chamada-cartao-estados");
  estados.append(elemento("span", "chamada-pastilha"), elemento("span", "chamada-pastilha"));
  topo.append(identidade, estados);

  // ---- meio: o sinal ----
  const sync = elemento("div", "chamada-cartao-sync");
  const linha = elemento("div", "chamada-cartao-linha");
  const numero = elemento("span", "chamada-cartao-numero");
  numero.append(
    elemento("span", "chamada-cartao-marca"),
    elemento("span", "chamada-cartao-valor"),
  );
  linha.append(elemento("span", "rotulo", "SINAL"), numero);

  const barra = elemento("span", "chamada-cartao-barra");
  // A barra é a mesma medida que o número ao lado, em forma. Lida em voz alta
  // ela é uma parede de blocos.
  barra.setAttribute("aria-hidden", "true");
  sync.append(linha, barra);

  cartao.append(topo, sync, controleDeVolume());
  return cartao;
}

/**
 * O controle de volume: `−`, dez células e `+`.
 *
 * O conserto de usabilidade que o v3 traz para esta tela. No v2 o volume por
 * pessoa era um deslizante que só aparecia quando o ponteiro passava por cima
 * da linha do roster — e um controle que só existe no hover é um controle
 * escondido: fora do caminho de quem usa teclado, invisível em toque, e
 * desconhecido de quem nunca passou o mouse ali por acaso.
 *
 * `−` é `U+2212` e `+` é ASCII, e os dois **existem** na face embarcada
 * (inventário §5): são os únicos glifos desta tela que podem ser digitados em
 * vez de desenhados.
 */
function controleDeVolume() {
  const volume = elemento("div", "chamada-volume");
  volume.append(elemento("span", "rotulo", "VOLUME"));

  const menos = elemento("button", "chamada-vol-passo", "−");
  menos.type = "button";
  menos.dataset.volAcao = "menos";

  const celas = elemento("div", "chamada-vol-celas");
  for (let indice = 0; indice < CELULAS_DE_VOLUME; indice += 1) {
    const cela = elemento("button", "chamada-vol-cela");
    cela.type = "button";
    cela.dataset.volAcao = "cela";
    cela.dataset.volAlvo = String(((indice + 1) * VOLUME_MAXIMO) / CELULAS_DE_VOLUME);
    celas.append(cela);
  }

  const mais = elemento("button", "chamada-vol-passo", "+");
  mais.type = "button";
  mais.dataset.volAcao = "mais";

  volume.append(menos, celas, mais, elemento("span", "chamada-vol-valor"));
  return volume;
}

/**
 * Os valores de um cartão, escritos por cima do esqueleto.
 *
 * Duas coisas que esta função não faz, e que o comp faz: decidir a faixa da
 * sincronia — ela chega pronta do core, e uma casca que soubesse que 85 é
 * nominal discordaria do terminal no dia em que um dos dois fosse atualizado —
 * e desenhar amplitude por pessoa, que não atravessa a fronteira.
 */
function pintarCartao(cartao, pessoa, temAudio) {
  cartao.dataset.faixa = pessoa.sync_band;
  cartao.dataset.fala = pessoa.speaking ? "sim" : "nao";

  cartao.querySelector(".chamada-avatar").textContent = iniciaisDoCartao(pessoa.nickname);
  cartao.querySelector(".chamada-cartao-nome").textContent =
    pessoa.nickname + (pessoa.is_self ? " (você)" : "");
  // O estado em frase, ao lado do estado em palavra, e sempre visível: é o dado
  // mais importante do cartão, e não havia versão desta tela em que escondê-lo
  // fosse defensável.
  cartao.querySelector(".nota").textContent = fraseDoEstado(pessoa);

  // A palavra, e não só a cor. `specs/06-clientes-gui.md` proíbe informação
  // transmitida só por cor, e o estado de quem está na sala é informação: o
  // halo laranja de quem transmite e a pastilha dizem a mesma coisa, e são
  // duas porque uma delas não chega a quem não vê a cor.
  //
  // `OUVINDO` some quando a pessoa está com o som desligado, e só então: as
  // duas pastilhas ficam lado a lado, e `OUVINDO · SEM SOM` é uma contradição
  // lida em meio segundo. Quem está surdo e calado tem um estado só a
  // declarar, e é o segundo.
  const pastilhas = cartao.querySelectorAll(".chamada-pastilha");
  const microfone = pessoa.muted
    ? "MUDO"
    : pessoa.speaking
      ? "FALANDO"
      : pessoa.total_isolation
        ? ""
        : "OUVINDO";
  pastilhas[0].textContent = microfone;
  pastilhas[0].hidden = microfone === "";
  pastilhas[0].dataset.estado = pessoa.muted ? "at" : pessoa.speaking ? "fala" : "escuta";

  // O isolamento total não existe no comp e existe no produto. Segunda
  // pastilha, e não uma troca da primeira: estar surdo e estar transmitindo são
  // dois fatos ao mesmo tempo, e um deles apagando o outro esconderia metade.
  pastilhas[1].textContent = "SEM SOM";
  pastilhas[1].dataset.estado = "surdo";
  pastilhas[1].hidden = !pessoa.total_isolation;

  cartao.querySelector(".chamada-cartao-marca").textContent = marcaSync(pessoa.sync_band);
  // Inteiro, e não `98.4`: `signal` é `u8` em todo ponto onde existe, e a
  // casa decimal do comp seria precisão inventada no último passo.
  cartao.querySelector(".chamada-cartao-valor").textContent = String(pessoa.signal);
  cartao.querySelector(".chamada-cartao-barra").textContent = blocos(
    pessoa.signal,
    BLOCOS_DA_BARRA,
  );

  // Baixar o próprio volume não faz nada, e sem áudio não há o que baixar. O
  // controle some nos dois casos em vez de ficar desabilitado: um botão morto é
  // uma pergunta ("por que não posso?") onde não havia pergunta nenhuma.
  const volume = cartao.querySelector(".chamada-volume");
  volume.hidden = pessoa.is_self || !temAudio;
  if (!volume.hidden) pintarVolume(cartao, pessoa.nickname);
}

/**
 * As células, o número e os nomes acessíveis do controle de volume.
 *
 * O valor é o que **esta janela mandou** — `set_volume` escreve e nada lê de
 * volta —, e é por isso que ele é a posição de um controle e não uma medida:
 * ninguém está dizendo aqui quanto o outro lado está tocando, e sim onde este
 * botão está. A lembrança mora em `volumes`, que a tela de operação divide.
 */
function pintarVolume(cartao, apelido) {
  const valor = volumes.get(apelido) ?? VOLUME_NORMAL;
  const cheias = Math.round((valor / VOLUME_MAXIMO) * CELULAS_DE_VOLUME);

  const celas = cartao.querySelectorAll(".chamada-vol-cela");
  celas.forEach((cela, indice) => {
    cela.dataset.cheia = indice < cheias ? "sim" : "nao";
    cela.setAttribute("aria-label", `pôr o volume de ${apelido} em ${cela.dataset.volAlvo}%`);
  });

  const passos = cartao.querySelectorAll(".chamada-vol-passo");
  passos[0].setAttribute("aria-label", `diminuir o volume de ${apelido}`);
  passos[1].setAttribute("aria-label", `aumentar o volume de ${apelido}`);

  cartao.querySelector(".chamada-vol-valor").textContent = `${valor}%`;
}

/**
 * As iniciais de um apelido, para o avatar do cartão.
 *
 * `NUNES.S` vira `IS`, `Daniel` vira `MI`. Separado por ponto, espaço, traço ou
 * sublinhado, que é como um apelido se divide em partes neste produto.
 */
function iniciaisDoCartao(apelido) {
  const partes = apelido.split(/[\s._-]+/).filter((parte) => parte.length > 0);
  if (partes.length === 0) return "";
  if (partes.length === 1) return partes[0].slice(0, 2).toUpperCase();
  return (partes[0][0] + partes[1][0]).toUpperCase();
}

/**
 * O estado de uma pessoa, em frase.
 *
 * A mesma informação da pastilha, na língua de quem nunca leu o manual — é o
 * que `LEGENDAS SIMPLES` é (inventário §2). O microfone vem antes porque calar
 * é o que impede alguém de ser ouvido; o isolamento entra na mesma frase porque
 * são dois fatos ao mesmo tempo e escolher um esconderia o outro.
 */
function fraseDoEstado(pessoa) {
  if (pessoa.total_isolation) {
    if (pessoa.muted) return "está com o microfone desligado e não ouve ninguém";
    if (pessoa.speaking) return "está falando agora, e não ouve ninguém";
    return "não está ouvindo ninguém";
  }
  if (pessoa.muted) return "está com o microfone desligado";
  if (pessoa.speaking) return "está falando agora";
  return "está só ouvindo";
}

/* `desenharMonitor` saiu com a tela.
 *
 * Ela escrevia `QUEM FALA` numa coluna própria da chamada. A coluna de pessoas
 * — que a comp mantém à direita e que agora fica visível **junto** com a grade,
 * porque a grade deixou de tomar a janela — já marca quem está falando, por
 * pessoa e não numa lista à parte. */

// -------------------------------------------------------------------- eventos


/* `desenharEventosDaChamada` e `registrarEventoDaChamada` saíram com a tela.
 *
 * Elas mantinham um `EVENTOS` que dizia de si mesmo «desde que esta janela
 * abriu; não há registro anterior» — uma lista que nascia vazia toda vez e não
 * sobrevivia a um recarregar. A comp não a traz, e o que ela mostrava de útil
 * (as quedas, os avisos do núcleo) chega pela faixa de alerta, que é anunciada
 * e não precisa de uma tela aberta para ser vista. */

/* `desenharAcoesDaChamada` saiu na 0.9.0.
 *
 * Ela pintava `chamada-at`, `chamada-surdez`, `chamada-compartilhar` e
 * `chamada-ejetar` — microfone, isolamento, compartilhar e sair do servidor —
 * numa fileira própria da tela de chamada. A comp dissolve aquela tela e põe
 * microfone e isolamento no **operador**, que já os tinha; sair do servidor já
 * era `botao-desconectar`; e o compartilhar foi para o cabeçalho do palco, ao
 * lado de `TELA CHEIA` — a comp não o desenha em lugar nenhum, e a fileira do
 * operador, onde ele parou primeiro, o deixava na tela o tempo todo, inclusive
 * fora da chamada, onde não há palco que receba o que ele produz.
 *
 * Quatro botões que existiam em dois lugares viraram quatro em um. */

// ---------------------------------------------------------------- navegação

/**
 * Mostra a grade da sala **no lugar da conversa**.
 *
 * # O que mudou na 0.9.0, e por quê
 *
 * Isto era `abrirChamada`, e abria uma **tela**: `tela-sessao` sumia inteira e
 * `tela-chamada` tomava a janela, com barra, ações e registro próprios. A comp
 * dissolve aquela tela — a grade é uma das duas vistas da coluna do meio, e as
 * outras três colunas continuam onde estavam.
 *
 * O ganho não é de arrumação. Trocar de tela apagava a lista de canais, o
 * roster e a telemetria; quem entrava na chamada perdia de vista quem estava
 * em que sala, que é exatamente o que se quer saber ao entrar numa. E o
 * caminho de volta era um botão que existia só porque a ida tinha sido larga
 * demais.
 */
async function abrirChamada() {
  $("vista-conversa").hidden = true;
  $("vista-chamada").hidden = false;
  // Antes do desenho, porque é ela que decide se o botão de compartilhar
  // existe: desenhar primeiro e esconder depois é um botão que ninguém viu
  // chegar nem sair.
  await conferirPermissaoDeTela();
  await atualizarChamada();
}

/**
 * Volta para a conversa **sem sair da sala**.
 *
 * Metade da distinção que o v3 traz (inventário §7.1) e que a comp mantém:
 * trocar de vista não é sair da sala. Um alterna o que a coluna mostra, o
 * outro chama `eject_plug`, e a nota de cada um diz qual é qual.
 */
function fecharChamada() {
  $("vista-chamada").hidden = true;
  $("vista-conversa").hidden = false;
  // O registro não foi redesenhado enquanto esteve escondido —
  // `desenharMensagens` sai cedo quando a lista está sem layout, porque ali não
  // dá para saber se a pessoa estava lendo o histórico ou acompanhando o fim.
  // Redesenhar agora é o outro lado desse acordo, e sem ele o canal volta
  // parado no instante em que a grade abriu.
  atualizar().catch((falha) => console.warn("voltar da grade:", falha));
}

/**
 * Volta à conversa sem devolver foco a ninguém.
 *
 * A sessão pode acabar com a grade aberta — um operador derruba, o enlace
 * descarrega — e `mostrarFim` escolhe a tela seguinte por conta própria.
 * Deixar a grade de pé faria a sessão seguinte abrir mostrando a sala da
 * anterior.
 */
function abandonarChamada() {
  $("vista-chamada").hidden = true;
  $("vista-conversa").hidden = false;
}

/**
 * Puxa o snapshot e redesenha.
 *
 * Sem sessão isto falha, e falhar não é aviso de nada: a tela desenha o vazio e
 * espera o `mostrarFim` que já está a caminho.
 */
async function atualizarChamada() {
  let snapshot = null;
  try {
    snapshot = await invoke("snapshot");
  } catch (falha) {
    if (falha !== "NotConnected") console.warn("snapshot:", falha);
  }
  desenharChamada(snapshot);
}

// ------------------------------------------------------------------- ligação


/**
 * O alternador da comp: a grade da sala **no lugar da conversa**, e de volta.
 *
 * Um botão só para os dois sentidos, e o rótulo diz para onde ele leva — não
 * onde se está. `CHAMADA` quando se está na conversa, `CONVERSA` quando se
 * está na grade. Dois botões seriam um deles sempre inútil.
 */
$("operador-vista").addEventListener("click", () => {
  const naGrade = !$("vista-chamada").hidden;
  if (naGrade) {
    fecharChamada();
  } else {
    abrirChamada().catch((falha) => console.warn("abrir a grade:", falha));
  }
  $("operador-vista").textContent = naGrade ? "CHAMADA" : "CONVERSA";
});

/**
 * Sair da sala de voz — e **só** dela.
 *
 * O par do alternador acima, e a distinção entre os dois é o que o
 * `as_duas_saidas_continuam_dizendo_qual_delas_larga_a_sala` prende: um troca
 * o que se vê, o outro para de ouvir e de falar. A conexão com o servidor não
 * é tocada por nenhum dos dois; quem sai do servidor é o `botao-desconectar`.
 */
$("operador-sair").addEventListener("click", async () => {
  try {
    await invoke("eject_plug");
  } catch (falha) {
    console.warn("eject_plug:", falha);
  }
  await atualizar();
});
$("chamada-compartilhar").addEventListener("click", () => {
  abrirCompartilhar().catch((falha) => console.warn("compartilhar:", falha));
});



/**
 * `SAIR DA SALA` — a outra metade da distinção do §7.1.
 *
 * Este é o `eject_plug`, e é o único dos dois botões do rodapé que sai da sala
 * de voz. Ele devolve para os canais depois, porque fora da sala esta tela não
 * tem grade nenhuma para desenhar e ficar nela mostraria o vazio de uma sala
 * que a pessoa acabou de deixar de propósito.
 */

/**
 * O volume, por delegação.
 *
 * Um ouvinte na grade e não um por botão: os cartões são repintados a cada meio
 * segundo e um ouvinte por botão teria que ser registrado de novo a cada
 * cartão novo. O `−`, o `+` e cada célula dizem no `dataset` o que fazem.
 *
 * A célula pinta na hora, antes da volta de IPC, e é o certo: o que ela mostra
 * é a posição de um controle que esta janela acabou de mover, não a leitura de
 * uma medida — som é o que confirma se o volume ficou onde devia.
 */
$("chamada-grade").addEventListener("click", (evento) => {
  const alvo = evento.target.closest("[data-vol-acao]");
  if (!alvo) return;

  const cartao = alvo.closest(".chamada-cartao");
  const apelido = cartao.dataset.pessoa;
  const atual = volumes.get(apelido) ?? VOLUME_NORMAL;

  const pedido =
    alvo.dataset.volAcao === "menos"
      ? atual - PASSO_DE_VOLUME
      : alvo.dataset.volAcao === "mais"
        ? atual + PASSO_DE_VOLUME
        : Number(alvo.dataset.volAlvo);
  const valor = Math.max(0, Math.min(VOLUME_MAXIMO, pedido));

  volumes.set(apelido, valor);
  pintarVolume(cartao, apelido);
  invoke("set_volume", { nickname: apelido, percent: valor }).catch((falha) => {
    console.warn("set_volume:", falha);
  });
});

// Escape fecha, que é o que uma tela alcançada de dentro de outra faz. Só com
// ela na frente, ou engoliria a tecla de quem está fechando uma busca.
window.addEventListener("keydown", (evento) => {
  // E só com nada por cima. Toda caixa desta janela fecha no Escape, e as duas
  // que se abrem daqui — a ajuda e o compartilhamento — registram o ouvinte
  // delas no mesmo `window`: sem esta conferência, um Escape fecharia a caixa
  // **e** a chamada por baixo dela, e quem só queria voltar da caixa acabaria
  // nos canais de texto. `role="dialog"` e não uma lista de `id`: uma terceira
  // caixa amanhã já entra coberta.
  const porCima = document.querySelector('[role="dialog"]:not([hidden])');
  if (evento.key === "Escape" && !$("vista-chamada").hidden && !porCima) {
    evento.preventDefault();
    fecharChamada();
  }
});

/**
 * Os avisos do core, guardados enquanto passam.
 *
 * Segundo ouvinte do mesmo canal, e de propósito: a sessão redesenha com o
 * evento, e isto **lembra** dele. O aviso é a única coisa que o core levanta
 * com sujeito e motivo — os outros quatro eventos dizem que algo mudou e não o
 * quê —, então é o único que vira linha de histórico sem ser inventado.
 *
 * Registrado no topo do arquivo e não ao abrir a tela: a lista tem que valer
 * para a sessão inteira, ou o painel só saberia do que aconteceu enquanto
 * alguém estava olhando para ele.
 */
listen("seele://event", (evento) => {
  const payload = evento.payload;
  if (!payload || typeof payload !== "object" || !payload.NoticeRaised) return;

  const aviso = payload.NoticeRaised.notice;
});

// Quem fala muda a cada instante, e é a coisa viva desta tela. Meio segundo, o
// mesmo da telemetria da operação, e só com a tela na frente — a operação
// desliga o laço dela pela mesma condição, então as duas nunca puxam o snapshot
// ao mesmo tempo.
setInterval(() => {
  if (!$("vista-chamada").hidden) atualizarChamada();
}, 500);
