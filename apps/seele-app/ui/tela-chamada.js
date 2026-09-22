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

  desenharORodapeDoOperador(voice_room);
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
 *   2. **quantas pessoas recebem**, como o servidor conta — `ScreenViewers`, o
 *      mesmo N pelo qual ele divide o teto no §5.1;
 *   3. **os controles desta transmissão**: o som dela, separado do das pessoas,
 *      e parar de assistir.
 *
 * # O que saiu daqui no R20
 *
 * O item 2 era «o que está saindo agora, ao lado do que foi pedido» (§5), e a
 * coluna da esquerda nunca teve número: nada nesta ponte mede resolução, quadros
 * ou banda. Três caixas escreviam «ainda não há medida» sobre o vídeo, para
 * sempre. Elas voltam quando houver medida de verdade, ligadas à transmissão
 * certa — ver a nota em `TelaEmCurso`.
 *
 * A contagem também mudou de fonte: era o roster desta máquina menos quem
 * compartilha, e incluía quem escolheu não assistir.
 *
 * **`snapshot.tela` é a que esta pessoa está assistindo**, e `minha_transmissao`
 * é a que ela está mandando. As duas podem existir ao mesmo tempo, e antes do
 * ADR 0054 havia um campo só, preenchido com a primeira transmissão da sala.
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
      // **A orientação aponta para onde o botão está** — U28.
      //
      // «O estado vazio diz "Use COMPARTILHAR, aqui em cima", mas o botão está
      // na base da coluna esquerda.» A frase foi escrita quando o botão ficava
      // no cabeçalho, e ficou quando ele mudou de lugar: uma instrução que
      // aponta para o canto errado é pior que nenhuma, porque quem a segue
      // conclui que o botão não existe.
      //
      // O botão de verdade fica ao lado desta frase agora — ver `palco-vazio`
      // em `index.html` —, e por isso ela deixa de descrever um caminho.
      ? "Nada está sendo transmitido nesta sala."
      : "Esta versão não sabe compartilhar tela desta máquina. A de outra pessoa apareceria aqui.";
    // O botão fica ao lado da frase, e só onde ele tem o que fazer: numa sala,
    // e com o controle de tela disponível nesta máquina.
    $("palco-compartilhar").hidden = !voice_room || !temControleDeTela();
    $("palco-parada").hidden = true;
    $("palco-controles").hidden = true;
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

  // Só a contagem, e ela é a do **servidor**: `ScreenViewers` conta assinaturas
  // de verdade. A resolução saiu daqui junto com as três caixas de métrica — ela
  // nunca foi medida, e `0p` era o que a ponte mandava. Ver R20.
  //
  // A contagem que estava aqui era o roster desta máquina menos quem
  // compartilha, e incluía quem escolheu não assistir: duas contagens para a
  // mesma pergunta, discordando. Ver `TelaEmCurso::espectadores`.
  $("palco-razao").textContent = assistindo(tela.espectadores);

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

  desenharControlesDoPalco(tela);
  desenharSomDoPalco().catch((falha) => console.warn("som do palco:", falha));
  botaoDeCinema(true);
}

/**
 * A barra compacta do palco: o som desta transmissão, e parar de assistir.
 *
 * # O que saiu daqui, e por quê
 *
 * `desenharNumerosDoPalco` escrevia três caixas — resolução, quadros, banda —
 * com o que está saindo ao lado do que foi pedido. **A coluna da esquerda nunca
 * teve número**: `medida` chegava `false` em toda transmissão, porque nada nesta
 * ponte alcança o codificador de quem transmite nem a recepção de quem assiste.
 * As três diziam «ainda não há medida desta transmissão», para sempre, numa área
 * própria sobre o vídeo. R20 da revisão da v15, e a decisão é a dela: tirar o
 * espaço vazio enquanto não há medição.
 *
 * O que fica é o que se sabe e o que se controla.
 */
function desenharControlesDoPalco(tela) {
  const controles = $("palco-controles");
  if (!controles) return;
  // Quem transmite não assiste a si mesmo pelo servidor: não há som chegando
  // para silenciar nem assinatura para largar. A barra some, e o palco vira só
  // o espelho do que está saindo.
  controles.hidden = Boolean(tela.e_minha);
}

/**
 * O volume e o mudo desta transmissão, lidos do Rust.
 *
 * Lidos e não guardados aqui: a escolha vive do lado que sobrevive à janela, e
 * duas cópias da mesma escolha são duas cópias que discordam depois de uma
 * recarga. Ver o ADR 0054.
 */
async function desenharSomDoPalco() {
  const mudo = $("palco-mudo");
  const volume = $("palco-volume");
  if (!mudo || !volume) return;
  let som = null;
  try {
    som = await invoke("som_da_tela");
  } catch (falha) {
    if (falha !== "NotConnected") console.warn("som da tela:", falha);
  }
  if (!som) return;
  mudo.setAttribute("aria-pressed", som.calada ? "true" : "false");
  mudo.textContent = som.calada ? "OUVIR" : "SILENCIAR";
  mudo.title = som.calada
    ? "voltar a ouvir esta transmissão; as pessoas continuam como estão"
    : "calar só esta transmissão; você continua ouvindo as pessoas";
  // O controle só é reescrito quando não está sendo arrastado: escrever nele a
  // cada volta de telemetria tiraria o polegar do dedo de quem o move.
  if (document.activeElement !== volume) {
    volume.value = String(Math.round(som.volume * 100));
  }
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

  // As iniciais **e** o retrato: as primeiras são o que se vê de quem não pôs
  // imagem, e `vestirAvatar` as substitui quando há uma. A grade da chamada era
  // a terceira superfície que a comp desenha com avatar e a única que continuou
  // só com letras — «ícone do usuário não aparece na chamada».
  const avatar = cartao.querySelector(".chamada-avatar");
  avatar.textContent = iniciaisDoCartao(pessoa.nickname);
  vestirAvatar(avatar, pessoa.id);
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
 * era o `+` da trilha, que confirma antes; e o compartilhar deu duas voltas —
 * a comp não o desenha em lugar nenhum. Parou primeiro na fileira do operador,
 * onde ficava na tela o tempo todo, inclusive fora de sala; foi para o cabeçalho
 * do palco, onde só existia com a grade aberta; e voltou ao operador
 * **condicionado à sala**, que é a forma que faltava — ver `desenharORodapeDoOperador`.
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
  mostrarDestinoNativo();
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
 * outro chama `leave_voice_room`, e a nota de cada um diz qual é qual.
 */
function fecharChamada() {
  mostrarDestinoNativo();
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
 * Os dois botões do rodapé do operador, e os dois mudam de ofício com a sala.
 *
 * ---- o que eram ----
 *
 * `SAIR DA SALA` e um alternador `CONVERSA`/`CHAMADA`, sempre os dois, sempre
 * iguais. O alternador existia porque a coluna do meio mostra uma coisa de cada
 * vez, e não havia outro jeito de escolher qual.
 *
 * ---- por que mudaram ----
 *
 * Agora há: a navegação passou para as listas da esquerda — clicar numa sala
 * mostra a grade dela, clicar num canal mostra a conversa. Com isso o alternador
 * virou um terceiro caminho para o que dois cliques já faziam, e o pior dos
 * três, porque não diz **de qual** sala nem **de qual** canal está falando.
 *
 * No lugar dele, dentro de uma sala, vai `COMPARTILHAR`. Ele existia só no
 * cabeçalho da grade, que é visível apenas quando já se está olhando a grade —
 * então quem estivesse lendo um canal, dentro de uma sala, não tinha por onde
 * começar a compartilhar sem antes trocar de vista.
 *
 * E fora de sala nenhuma os dois somem ou trocam: não há sala de onde sair, e a
 * saída que existe ali é do servidor. Um botão que nomeia um lugar onde não se
 * está é um botão que promete não fazer nada — foi o argumento que tirou
 * `CHAMADA` de cima da chamada, e ele vale igual aqui.
 *
 * ---- o estado mora no botão ----
 *
 * Os ouvintes são registrados uma vez, no carregamento, e o que decide o que
 * eles fazem é o `dataset` que este desenho escreve. É o mesmo idioma do rótulo
 * lido da tela que o alternador já usava: um estado guardado em variável de
 * módulo fica velho sempre que alguma outra coisa muda a sala, e há caminhos que
 * mudam — entrar por um clique na lista, ser expulso, o servidor cair.
 */
function desenharORodapeDoOperador(voice_room) {
  const dentro = voice_room != null;

  const sair = $("operador-sair");
  sair.dataset.alvo = dentro ? "sala" : "servidor";
  sair.textContent = dentro ? "SAIR DA SALA" : "SAIR DO SERVIDOR";
  sair.title = dentro
    ? `sair de ${voice_room.name}: você para de ouvir e de falar nesta sala`
    : "sair deste servidor e voltar à tela de entrada";

  const outro = $("operador-vista");
  // `hidden`, e não um estilo: fora de sala este botão não tem ofício nenhum, e
  // um botão desabilitado prometeria que existe um jeito de habilitá-lo aqui.
  outro.hidden = !dentro;
  if (dentro) {
    outro.textContent = "COMPARTILHAR";
    outro.title = `mostrar uma tela ou uma janela para quem está em ${voice_room.name}`;
  }
}

// **Duas portas, um comando** — U28.
//
// O botão do rodapé e o que fica ao lado do estado vazio da chamada chamam a
// mesma função. Não é um segundo caminho: é o mesmo, no lugar onde a pessoa
// está olhando quando precisa dele. Dois comandos com o mesmo rótulo é que
// seria um problema — duas maneiras de fazer a mesma coisa, discordando no dia
// em que uma delas mudar.
for (const porta of ["operador-vista", "palco-compartilhar"]) {
  $(porta).addEventListener("click", () => {
    abrirCompartilhar().catch((falha) => console.warn("compartilhar:", falha));
  });
}

/**
 * Sair — da sala, ou do servidor, conforme onde se está.
 *
 * **Sem caixa de confirmação para o caso comum, e com ela para um só.** A regra
 * está escrita em `pedirTrocaDeServidor`: uma caixa que repete o rótulo do botão
 * é a caixa que treina a apertar duas vezes sem ler. `SAIR DA SALA` e
 * `SAIR DO SERVIDOR` dizem o que fazem, e o que se perde nos dois se recupera
 * entrando de novo.
 *
 * O caso que não se recupera é hospedar: o servidor que este computador põe no
 * ar cai junto com quem o hospeda, e todo mundo que estiver dentro sai. Isso o
 * rótulo não diz, e é exatamente a parte que a caixa existe para escrever.
 */
$("operador-sair").addEventListener("click", async () => {
  if ($("operador-sair").dataset.alvo === "sala") {
    try {
      await invoke("leave_voice_room");
    } catch (falha) {
      console.warn("leave_voice_room:", falha);
    }
    await atualizar();
    return;
  }

  const daqui = nomeDesteServidor();
  if (await hospedandoAqui()) {
    abrirConfirmacao(
      "SAIR DO SERVIDOR",
      consequenciaDeIrParaAEntrada(daqui, true),
      `SAIR DE ${daqui}`,
      sairDoServidorParaAEntrada,
    );
    return;
  }
  await sairDoServidorParaAEntrada();
});





/**
 * `SAIR DA SALA` — a outra metade da distinção do §7.1.
 *
 * Este é o `leave_voice_room`, e é o único dos dois botões do rodapé que sai da sala
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

// ---------------------------------------------------- os controles do palco
//
// R17 e R20. Registrados uma vez, no carregamento: o palco é redesenhado duas
// vezes por segundo, e um ouvinte por desenho seria um ouvinte jogado fora a
// cada volta.

$("palco-mudo").addEventListener("click", async () => {
  const botao = $("palco-mudo");
  const calada = botao.getAttribute("aria-pressed") !== "true";
  try {
    await invoke("calar_a_tela", { calada });
  } catch (falha) {
    console.warn("calar a tela:", falha);
  }
  await desenharSomDoPalco();
});

// `input` e não `change`: o volume é um controle contínuo, e esperar a soltura
// faria a pessoa arrastar no escuro.
$("palco-volume").addEventListener("input", (evento) => {
  const volume = Number(evento.target.value) / 100;
  invoke("ajustar_volume_da_tela", { volume }).catch((falha) =>
    console.warn("volume da tela:", falha),
  );
  // Mexer no volume desfaz o mudo: é o gesto dizendo «quero ouvir isto». Sem
  // isto, arrastar o volume com o som calado não faria nada e a pessoa leria
  // o controle como quebrado.
  if (volume > 0 && $("palco-mudo").getAttribute("aria-pressed") === "true") {
    invoke("calar_a_tela", { calada: false })
      .then(desenharSomDoPalco)
      .catch((falha) => console.warn("calar a tela:", falha));
  }
});

// **«Parar de assistir», e o nome é a decisão.** Encerra vídeo e áudio daquela
// transmissão; a conversa de voz continua. Quem chama é `palco-imagem.js`, que
// é quem guarda a intenção — ver `pararDeVer`.
$("palco-parar-de-assistir").addEventListener("click", () => {
  window.dispatchEvent(new CustomEvent("seele:parar-de-assistir"));
});
