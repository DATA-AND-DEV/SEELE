// SEELE · Entry Plug — a tela de chamada (`#tela-chamada`).
//
// A grade de quem está no Cage: um cartão por piloto, com estado em palavra,
// sincronia e volume; e o monitor do Cage à direita. É a tela `chamada` do comp
// v3 (`.superpowers/sdd/comp-inventario-v3.md` §7).
//
// ---- o que este arquivo não decide ----
//
// Nada. A faixa de cada sincronia chega decidida do core (`SyncBand`), quem
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
// ser simples um travessão explicado é ruído: quem entra numa jaula quer saber
// quem está falando, não quais campos este protocolo ainda não carrega.
//
// Então saíram, em vez de ficarem vazios:
//
//   - **a onda por piloto.** `Telemetry.input_level` é escalar e é **nosso** —
//     amplitude por interlocutor não atravessa a fronteira e não é derivável de
//     nada que atravesse. Vinte e quatro barras paradas seriam vinte e quatro
//     afirmações de silêncio; oscilando, vinte e quatro mentiras animadas com o
//     nome de outra pessoa embaixo.
//   - **o atraso por piloto.** `Telemetry.rtt_ms` é o desta máquina até o
//     Dogma, um número só. Ele continua na telemetria da tela de operação, que
//     é onde ele é verdade.
//   - **a tag de subsistema** (`BALTHASAR·01`), que o protocolo não tem.
//   - **a rota e o cronômetro do plug**, pela mesma razão — ver `EM SESSÃO`,
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

/** Quantos blocos a barra de sincronia do cartão tem. */
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
 * O comp desenha cinco linhas de histórico — `IKARI.S entrou`,
 * `HORAKI.H saiu` — e o inventário §9.8 deixou em aberto quanto disso o Dogma
 * guarda. A resposta é: nada. `Event::RosterChanged` diz que o roster mudou,
 * nunca o quê, e não há histórico de entrada, saída nem A.T. Field em lugar
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
  const cage = snapshot ? snapshot.cages.find((c) => c.occupied_by_us) : null;

  desenharBarraDaChamada(snapshot, cage);
  desenharCartoes(snapshot, cage);
  desenharMonitor(snapshot, cage);
}

/** A barra de cima: o nome do Cage, o enlace e o relógio da sessão. */
function desenharBarraDaChamada(snapshot, cage) {
  const nome = $("chamada-nome");
  if (cage) {
    medido(nome, cage.name);
  } else {
    naoMedido(nome, snapshot ? "nenhum plug inserido" : "sem sessão");
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

  // `duracao` no comp é o tempo de chamada. O protocolo não carrega quando o
  // plug entrou, e um cronômetro que começasse a contar ao abrir esta tela
  // diria que ele entrou agora — que é pior que não dizer nada. O que existe é
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
 * Um cartão por piloto, ou a frase do vazio.
 *
 * Os cartões são **repintados**, e não refeitos, quando já existem. Não é
 * economia: esta tela redesenha duas vezes por segundo e o cartão agora tem
 * botões dentro. Refazer os nós a cada volta tiraria o foco de quem navega por
 * teclado a cada meio segundo, e um clique que atravessasse a troca sumiria sem
 * erro nenhum — o `mouseup` cairia num nó que não é mais o do `mousedown`.
 */
function desenharCartoes(snapshot, cage) {
  const grade = $("chamada-grade");

  if (!cage || cage.pilots.length === 0) {
    const frase = !snapshot
      ? "SEM SESSÃO"
      : cage
        ? "ESTE CAGE ESTÁ VAZIO"
        : "NENHUM PLUG INSERIDO";
    if (grade.children.length === 1 && grade.children[0].textContent === frase) return;
    repovoar(grade, [elemento("li", "chamada-vazio", frase)]);
    return;
  }

  const existentes = new Map();
  for (const cartao of grade.children) {
    if (cartao.dataset.piloto) existentes.set(cartao.dataset.piloto, cartao);
  }

  const cartoes = cage.pilots.map((piloto) => {
    const cartao = existentes.get(piloto.nickname) ?? cartaoDoPiloto(piloto);
    pintarCartao(cartao, piloto, snapshot.audio_available);
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
function cartaoDoPiloto(piloto) {
  const cartao = elemento("li", "chamada-cartao");
  cartao.dataset.piloto = piloto.nickname;

  // ---- topo: iniciais, nome, o estado em frase, o estado em palavra ----
  const topo = elemento("div", "chamada-cartao-topo");
  const identidade = elemento("div", "chamada-cartao-identidade");

  // As iniciais são decoração: elas repetem o nome que está do lado, em forma.
  // Anunciá-las seria ler o nome duas vezes.
  const avatar = elemento("span", "chamada-avatar");
  avatar.setAttribute("aria-hidden", "true");

  const quem = elemento("div", "chamada-cartao-quem");
  quem.append(elemento("span", "chamada-cartao-nome"), elemento("span", "dica"));
  identidade.append(avatar, quem);

  const estados = elemento("div", "chamada-cartao-estados");
  estados.append(elemento("span", "chamada-pastilha"), elemento("span", "chamada-pastilha"));
  topo.append(identidade, estados);

  // ---- meio: sincronia ----
  const sync = elemento("div", "chamada-cartao-sync");
  const linha = elemento("div", "chamada-cartao-linha");
  const numero = elemento("span", "chamada-cartao-numero");
  numero.append(
    elemento("span", "chamada-cartao-marca"),
    elemento("span", "chamada-cartao-valor"),
  );
  linha.append(elemento("span", "rotulo", "SINCRONIZAÇÃO"), numero);

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
 * e desenhar amplitude por piloto, que não atravessa a fronteira.
 */
function pintarCartao(cartao, piloto, temAudio) {
  cartao.dataset.faixa = piloto.sync_band;
  cartao.dataset.fala = piloto.speaking ? "sim" : "nao";

  cartao.querySelector(".chamada-avatar").textContent = iniciaisDoCartao(piloto.nickname);
  cartao.querySelector(".chamada-cartao-nome").textContent =
    piloto.nickname + (piloto.is_self ? " (você)" : "");
  // O estado em frase, ao lado do estado em palavra: é a camada
  // `LEGENDAS SIMPLES` do v3 aplicada ao dado mais importante do cartão.
  cartao.querySelector(".dica").textContent = fraseDoEstado(piloto);

  // A palavra, e não só a cor. `specs/05-cliente-tui.md` proíbe informação
  // transmitida só por cor, e o estado de quem está na sala é informação: o
  // halo laranja de quem transmite e a pastilha dizem a mesma coisa, e são
  // duas porque uma delas não chega a quem não vê a cor.
  //
  // `OUVINDO` some quando a pessoa está com o som desligado, e só então: as
  // duas pastilhas ficam lado a lado, e `OUVINDO · SEM SOM` é uma contradição
  // lida em meio segundo. Quem está surdo e calado tem um estado só a
  // declarar, e é o segundo.
  const pastilhas = cartao.querySelectorAll(".chamada-pastilha");
  const microfone = piloto.at_field
    ? "MUDO"
    : piloto.speaking
      ? "FALANDO"
      : piloto.total_isolation
        ? ""
        : "OUVINDO";
  pastilhas[0].textContent = microfone;
  pastilhas[0].hidden = microfone === "";
  pastilhas[0].dataset.estado = piloto.at_field ? "at" : piloto.speaking ? "fala" : "escuta";

  // O isolamento total não existe no comp e existe no produto. Segunda
  // pastilha, e não uma troca da primeira: estar surdo e estar transmitindo são
  // dois fatos ao mesmo tempo, e um deles apagando o outro esconderia metade.
  pastilhas[1].textContent = "SEM SOM";
  pastilhas[1].dataset.estado = "surdo";
  pastilhas[1].hidden = !piloto.total_isolation;

  cartao.querySelector(".chamada-cartao-marca").textContent = marcaSync(piloto.sync_band);
  // Inteiro, e não `98.4`: `sync_ratio` é `u8` em todo ponto onde existe, e a
  // casa decimal do comp seria precisão inventada no último passo.
  cartao.querySelector(".chamada-cartao-valor").textContent = String(piloto.sync_ratio);
  cartao.querySelector(".chamada-cartao-barra").textContent = blocos(
    piloto.sync_ratio,
    BLOCOS_DA_BARRA,
  );

  // Baixar o próprio volume não faz nada, e sem áudio não há o que baixar. O
  // controle some nos dois casos em vez de ficar desabilitado: um botão morto é
  // uma pergunta ("por que não posso?") onde não havia pergunta nenhuma.
  const volume = cartao.querySelector(".chamada-volume");
  volume.hidden = piloto.is_self || !temAudio;
  if (!volume.hidden) pintarVolume(cartao, piloto.nickname);
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
 * `IKARI.S` vira `IS`, `Misato` vira `MI`. Separado por ponto, espaço, traço ou
 * sublinhado, que é como um apelido se divide em partes neste produto.
 */
function iniciaisDoCartao(apelido) {
  const partes = apelido.split(/[\s._-]+/).filter((parte) => parte.length > 0);
  if (partes.length === 0) return "";
  if (partes.length === 1) return partes[0].slice(0, 2).toUpperCase();
  return (partes[0][0] + partes[1][0]).toUpperCase();
}

/**
 * O estado de um piloto, em frase.
 *
 * A mesma informação da pastilha, na língua de quem nunca leu o manual — é o
 * que `LEGENDAS SIMPLES` é (inventário §2). O microfone vem antes porque calar
 * é o que impede alguém de ser ouvido; o isolamento entra na mesma frase porque
 * são dois fatos ao mesmo tempo e escolher um esconderia o outro.
 */
function fraseDoEstado(piloto) {
  if (piloto.total_isolation) {
    if (piloto.at_field) return "está com o microfone desligado e não ouve ninguém";
    if (piloto.speaking) return "está falando agora, e não ouve ninguém";
    return "não está ouvindo ninguém";
  }
  if (piloto.at_field) return "está com o microfone desligado";
  if (piloto.speaking) return "está falando agora";
  return "está só ouvindo";
}

/** A coluna da direita: quem fala. */
function desenharMonitor(snapshot, cage) {
  const falando = $("chamada-falando-nome");
  const nomes = cage ? cage.pilots.filter((p) => p.speaking).map((p) => p.nickname) : [];

  if (!cage) {
    naoMedido(falando, snapshot ? "nenhum plug inserido" : "sem sessão");
    return;
  }
  // `NINGUÉM` e não travessão: um Cage em silêncio é uma medida, e o travessão
  // está reservado para o que este protocolo não sabe dizer.
  medido(falando, nomes.length === 0 ? "NINGUÉM" : nomes.join(" · "));
}

// -------------------------------------------------------------------- eventos

/**
 * Guarda uma linha do que aconteceu, e redesenha a lista.
 *
 * A hora é a desta máquina, e é a única possível: o que está sendo registrado é
 * o instante em que **esta janela** soube da coisa. `tom` separa aviso de
 * anotação, e a severidade que o decide chega decidida do core.
 */
function registrarEventoDaChamada(texto, tom) {
  EVENTOS_DA_CHAMADA.unshift({ hora: relogio(Date.now() / 1000), texto, tom });
  EVENTOS_DA_CHAMADA.length = Math.min(EVENTOS_DA_CHAMADA.length, EVENTOS_LEMBRADOS);
  desenharEventosDaChamada();
}

/** A lista de eventos, do mais novo para o mais velho. */
function desenharEventosDaChamada() {
  repovoar(
    $("chamada-eventos"),
    EVENTOS_DA_CHAMADA.map((evento) => {
      const item = elemento("li", "chamada-evento");
      item.dataset.tom = evento.tom;
      item.append(
        elemento("span", "chamada-evento-hora", evento.hora),
        elemento("span", "chamada-evento-texto", evento.texto),
      );
      return item;
    }),
  );
}

/** Os quatro botões da barra de ações. */
function desenharAcoesDaChamada(snapshot) {
  // O rótulo diz em que estado a coisa **está**, e não o que apertá-la vai
  // fazer; a dica diz o que apertá-la vai fazer. Um botão com o verbo no rótulo
  // é um botão que ninguém sabe ler quando volta a olhar para a tela — e uma
  // chave sem o verbo em lugar nenhum é uma chave que ninguém arrisca virar.
  const at = $("chamada-at");
  const calado = Boolean(snapshot?.at_field);
  at.dataset.ativo = calado ? "sim" : "nao";
  at.disabled = !snapshot;
  $("chamada-at-titulo").textContent = calado ? "MICROFONE DESLIGADO" : "MICROFONE LIGADO";
  $("chamada-at-dica").textContent = calado
    ? "clique para voltar a falar"
    : "clique para silenciar você — é o A.T. Field";

  const surdez = $("chamada-surdez");
  const surdo = Boolean(snapshot?.total_isolation);
  surdez.dataset.ativo = surdo ? "sim" : "nao";
  surdez.disabled = !snapshot;
  $("chamada-surdez-titulo").textContent = surdo ? "SOM DESLIGADO" : "SOM LIGADO";
  $("chamada-surdez-dica").textContent = surdo
    ? "clique para voltar a ouvir"
    : "clique para não ouvir ninguém — é o Isolamento total";

  $("chamada-ejetar").disabled = !snapshot;
}

// ---------------------------------------------------------------- navegação

/** Abre a chamada por cima da operação. */
async function abrirChamada() {
  // Antes do `hidden`, com a operação ainda na tela: depois dela sumir o foco
  // já é o `<body>` e não há mais o que lembrar. É o que devolve o botão
  // CHAMADA a quem o apertou, na hora de voltar.
  guardarFoco("tela-sessao");
  $("tela-sessao").hidden = true;
  $("tela-chamada").hidden = false;
  // E o foco só **depois** do desenho: `desenharAcoesDaChamada` é quem tira o
  // `disabled` da chave do microfone, e um botão desabilitado recusa o foco em
  // silêncio.
  await atualizarChamada();
  abrirTela("tela-chamada");
}

/**
 * Volta para as Linhas **sem tirar o plug**.
 *
 * Metade da distinção que o v3 traz (inventário §7.1): trocar de tela não é
 * sair do Cage. No protótipo os dois botões do rodapé chamam a mesma coisa e o
 * que os separa é só o que prometem; aqui um troca de tela e o outro chama
 * `eject_plug`, e a dica de cada um diz qual é qual.
 */
function fecharChamada() {
  guardarFoco("tela-chamada");
  $("tela-chamada").hidden = true;
  $("tela-sessao").hidden = false;
  // Devolve o CHAMADA do cabeçalho a quem o apertou. Quem chegou aqui pelo
  // Escape volta para onde estava, que é o que a tecla promete.
  voltarParaTela("tela-sessao");
  // A sessão não foi redesenhada enquanto esteve por baixo — `desenharMensagens`
  // sai cedo quando a lista está sem layout, porque ali não dá para saber se a
  // pessoa estava lendo o histórico ou acompanhando o fim. Redesenhar agora é o
  // outro lado desse acordo, e sem ele a Linha volta parada no instante em que
  // a chamada abriu.
  atualizar().catch((falha) => console.warn("voltar da chamada:", falha));
}

/**
 * Some sem devolver ninguém.
 *
 * A sessão pode acabar com esta tela aberta — um operador derruba, o enlace
 * descarrega — e `mostrarFim` escolhe a tela seguinte por conta própria.
 * Devolver para a operação ali reabriria uma sessão que acabou de terminar; não
 * fechar deixaria duas telas empilhadas, porque toda `.tela` tem a altura da
 * janela. Mesma razão e mesma forma que `abandonarDogma`.
 */
function abandonarChamada() {
  $("tela-chamada").hidden = true;
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
  desenharAcoesDaChamada(snapshot);
}

// ------------------------------------------------------------------- ligação

$("botao-chamada").addEventListener("click", abrirChamada);
$("chamada-voltar").addEventListener("click", fecharChamada);

$("chamada-at").addEventListener("click", async () => {
  const snapshot = await invoke("snapshot");
  const calar = !snapshot.at_field;
  await invoke("set_at_field", { on: calar });
  registrarEventoDaChamada(
    calar ? "você desligou o seu microfone" : "você ligou o seu microfone",
    "anotacao",
  );
  await atualizarChamada();
});

$("chamada-surdez").addEventListener("click", async () => {
  const snapshot = await invoke("snapshot");
  const calar = !snapshot.total_isolation;
  await invoke("set_total_isolation", { on: calar });
  registrarEventoDaChamada(
    calar ? "você desligou o som" : "você ligou o som de volta",
    "anotacao",
  );
  await atualizarChamada();
});

/**
 * `SAIR DA JAULA` — a outra metade da distinção do §7.1.
 *
 * Este é o `eject_plug`, e é o único dos dois botões do rodapé que sai do Cage.
 * Ele devolve para a Linha depois, porque sem plug esta tela não tem grade
 * nenhuma para desenhar e ficar nela mostraria o vazio de uma sala que a pessoa
 * acabou de deixar de propósito.
 */
$("chamada-ejetar").addEventListener("click", async () => {
  try {
    await invoke("eject_plug");
    registrarEventoDaChamada("você saiu da jaula", "anotacao");
  } catch (falha) {
    console.warn("eject_plug:", falha);
  }
  fecharChamada();
  await atualizar();
});

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
  const apelido = cartao.dataset.piloto;
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
  if (evento.key === "Escape" && !$("tela-chamada").hidden) {
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
  registrarEventoDaChamada(
    aviso.operator_text ?? AVISOS[aviso.reason] ?? "AVISO",
    // A severidade chega decidida do core. Esta tela não classifica nada: ela
    // só separa o que veio marcado como mais alto que informação.
    aviso.severity === "Info" ? "anotacao" : "aviso",
  );
});

// Quem fala muda a cada instante, e é a coisa viva desta tela. Meio segundo, o
// mesmo da telemetria da operação, e só com a tela na frente — a operação
// desliga o laço dela pela mesma condição, então as duas nunca puxam o snapshot
// ao mesmo tempo.
setInterval(() => {
  if (!$("tela-chamada").hidden) atualizarChamada();
}, 500);
