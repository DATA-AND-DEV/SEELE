// SEELE · a tela de entrada (`#tela-boot`).
//
// Onde você já esteve, o convite colado, e a conexão — mais hospedar aqui
// dentro, que é a mesma conexão com um servidor que este processo acabou de
// subir. Sai daqui para `#tela-auth`, que é onde o veredito da chave é lido
// antes de se entrar em sala de voz nenhuma.
//
// A coluna da esquerda é a apresentação do produto e não tem código: marca,
// três fatos, glossário. Tudo o que este arquivo move está na direita, mais a
// linha de CONEXÃO embaixo da marca. Do que o comp v2 animava nesta tela, nada
// tinha dado por trás — o `bootPct` que subia de 7 em 7 era um cronômetro de
// protótipo, e o registro de dez linhas carimbadas descrevia um stream de
// progresso que o protocolo não tem. O que esta tela move é o que ela sabe: a
// tentativa de conexão, pelo tempo real dela.

"use strict";

/* `subsistemas` saiu com a sequência de boot animada da tela antiga.
 *
 * Ela acendia uma marca por subsistema enquanto a conexão andava. A leitura de
 * boot da 0.9.0 é fixa e verdadeira — diz o que o produto é, e não o que está
 * acontecendo agora —, e o andamento da conexão continua sendo dito pelo
 * `boot-etapa`, que é uma frase e não um enfeite. */

/* `convitePendente` saiu na 0.9.0.
 *
 * Ele guardava o convite lido do campo da entrada até o momento de conectar — e
 * um convite guardado é um convite que pode ser aplicado ao servidor errado, que
 * é o que o `nenhum_convite_sobrevive_a_troca_de_servidor` cobrava em cada
 * caminho de troca.
 *
 * Agora o token viaja como argumento de `conectar`, dentro da mesma chamada em
 * que foi lido. Não há o que sobreviver a nada. */

/* `desenharVisitados` saiu com o formulário da entrada.
 *
 * A lista de visitados virou o diálogo `ONDE VOCÊ JÁ ESTEVE`, em
 * `camada-servidores.js`, e lá ela ganhou o que não tinha: o nome de quando se
 * esteve no servidor, há quanto tempo foi, e uma frase para quando está vazia. */

/* `lerConvite` e `limparConvite` saíram com o campo que liam.
 *
 * O `seele://` passou a ser aceito pelo mesmo campo que aceita o endereço, no
 * diálogo de servidores — porque as duas coisas respondem à mesma pergunta, e
 * duas caixas para ela eram duas chances de colar na errada. */


/**
 * Onde e como se entrou por último.
 *
 * `conectar()` sem argumento significa **de novo, no mesmo lugar** — é o que a
 * reconexão depois de uma queda pede, e o que a tela de fim chama. Sem esta
 * memória, aqueles caminhos teriam de carregar o endereço por conta própria, e
 * seriam três lugares guardando a mesma coisa.
 */
let ultimoAlvo = null;
let ultimoApelido = null;

/**
 * **Este `connect` é para o servidor que esta janela acabou de subir.**
 *
 * A `#tela-auth` existe para o momento TOFU do ADR 0003: quem chega a um
 * servidor de outra pessoa **olha** a impressão digital antes de entrar, e é
 * esse olhar que detecta alguém no meio do caminho. Hospedando não há meio do
 * caminho — o certificado foi gerado por este mesmo processo, nesta mesma
 * máquina, segundos antes, e a chave a conferir é a nossa contra a nossa.
 *
 * O relato de campo foi a pergunta certa: «se eu vou hospedar o server, por que
 * preciso ir pra tela onde mostra entrar no servidor?». Um pedágio que não
 * decide nada ensina a atravessar sem ler — e o dia em que ele decidir alguma
 * coisa, num servidor alheio de verdade, o dedo já vai estar treinado a passar
 * batido. A tela vale mais quando aparece menos.
 *
 * Uma bandeira e não um argumento de `conectar`: quem chama `conectar` de fora
 * — reconectar, trocar de servidor, um link de convite — está sempre indo a um
 * servidor que não é este, e não deveria precisar dizer isso.
 *
 * O nome é longo de propósito. Ele se chamava `hospedandoAqui`, que é o nome de
 * uma **função** que `tela-sessao.js` já tinha — e os catorze scripts desta
 * janela dividem um escopo global só. Um `let` colidindo com uma `function` de
 * outro arquivo não é um aviso: é `SyntaxError` no arquivo inteiro, que deixa
 * de carregar e leva junto tudo o que declarava.
 */
let subindoServidorAqui = false;

async function conectar(alvo, apelido, token) {
  ultimoAlvo = alvo ?? ultimoAlvo;
  ultimoApelido = apelido ?? ultimoApelido;
  // O token **não** é lembrado: um convite de uso único vale uma vez, e
  // reenviá-lo numa reconexão seria gastar de novo o que já foi gasto.
  if (!ultimoAlvo) {
    // Sem lugar nenhum a que voltar: quem chegou aqui apertou reconectar antes
    // de ter entrado em algum lugar, e o diálogo é onde se escolhe.
    abrirServidores().catch((falha) => console.warn("abrir servidores:", falha));
    return;
  }
  alvo = ultimoAlvo;
  // **O apelido desta máquina, quando esta chamada não trouxe um.**
  //
  // Ele era `ultimoApelido ?? ""`, e o `ultimoApelido` só existe depois de
  // alguém ter conectado a algum lugar nesta execução. Quem escreve o nome no
  // perfil da tela inicial e aperta `HOSPEDAR AQUI` não passou por lugar
  // nenhum: `hospedar` chama `conectar()` sem argumento, e o servidor recebia
  // vazio. «Coloquei meu nome no Windows na tela inicial e hospedei, o server
  // não puxou meu nome.»
  //
  // Aqui e não em `hospedar` porque aqui é o único lugar por onde **toda**
  // conexão passa — hospedar, reconectar, a trilha, o diálogo de conhecidos. O
  // diálogo já fazia esta leitura sozinho, e era a única porta que a fazia.
  apelido = (ultimoApelido ?? "").trim();
  if (apelido === "") {
    try {
      apelido = ((await invoke("apelido_local")) ?? "").trim();
    } catch (falha) {
      console.warn("apelido_local:", falha);
    }
  }
  // **Lembrado só quando é escolha de alguém**, e esta ordem é o conserto de
  // um defeito de campo que não tinha saída.
  //
  // A linha morava depois do recurso logo abaixo, e guardava o recurso junto.
  // O efeito: quem chegava sem nome escolhido tentava entrar como `pessoa`,
  // era recusada porque o nome já era de outra chave, ia ao perfil, escrevia o
  // nome de verdade e gravava — e a tentativa seguinte lia este cache, achava
  // `pessoa` guardado aqui, e **nunca mais consultava o perfil**. O nome novo
  // ficava gravado no disco sem nunca sair da máquina. A pessoa trocava de
  // apelido quantas vezes quisesse e recebia sempre a mesma recusa, num
  // servidor com cinco nomes — e a única saída era fechar o app, porque este
  // `let` só morre com ele.
  //
  // Um recurso não é escolha de ninguém e não tem o que fazer aqui. O cache
  // continua guardando o que **veio** de escolha: o apelido daquela visita que
  // o diálogo de conhecidos passa, e o da sessão que a troca de servidor passa.
  if (apelido !== "") ultimoApelido = apelido;
  // **O vazio segue para o Rust, que é quem sabe derivar um nome desta
  // máquina.** Ver `connect` no `main.rs`: o recurso deixou de ser a palavra
  // `pessoa`, igual para todo mundo e por isso garantida de colidir com a
  // segunda pessoa sem nome, e passou a ser `pessoa-` mais quatro caracteres da
  // impressão da chave daqui. A casca não tem essa impressão, e pedi-la por IPC
  // só para montar uma palavra seria uma volta a mais para chegar no mesmo.
  // **A bandeira é lida e apagada aqui, e não depois do `connect`.**
  //
  // Apagá-la só no caminho feliz a deixaria ligada quando o `connect` falha —
  // e a próxima conexão, essa a um servidor alheio de verdade, entraria sem a
  // conferência de identidade que é a razão de a `#tela-auth` existir. Uma
  // tentativa de hospedar que dá errado não pode virar permissão para a
  // seguinte.
  const nossoServidor = subindoServidorAqui;
  subindoServidorAqui = false;
  // **Os valores chegam por argumento desde a 0.9.0.**
  //
  // Eles vinham de dois campos desta tela, e a comp tirou os dois: o endereço
  // foi para o diálogo de servidores conhecidos, e o apelido para o perfil, que
  // o grava nesta máquina. Manter os campos escondidos só para carregá-los
  // seria guardar estado numa marcação que ninguém vê — e um campo invisível é
  // onde um valor errado sobrevive sem ser notado.
  const botao = $("botao-conectar");
  const erro = $("boot-erro");

  botao.disabled = true;
  erro.hidden = true;
  // A linha de CONEXÃO reporta enquanto a conexão acontece. Dura o tempo real
  // dela: `specs/06-clientes-gui.md` chama animação decorativa que atrasa o
  // usuário de falha de design.
  // A linha de etapa nasce vazia a cada tentativa: a de uma conexão anterior
  // descreveria uma travessia que já acabou.
  mostrarEtapa(null);

  try {
    // A entrada traz duas coisas: a tela, e o que a chave deste servidor acabou
    // de ser. A segunda vem do mesmo `connect` porque é lá que ela é decidida —
    // um ouvinte inscrito depois chegaria sempre tarde.
    const { snapshot, veredito } = await invoke("connect", {
      server: alvo,
      nickname: apelido,
      // Sempre com áudio. A caixa de «entrar com áudio» saiu com a tela antiga,
      // e quem não quer falar tem o microfone mudo no operador — que é
      // reversível, e é o que se procura quando se muda de ideia. Uma sessão
      // aberta sem áudio não ganha voz sem sair e entrar de novo.
      audio: true,
      // O token do convite, quando o link trouxe um. `join_secret` do outro
      // lado: a ponte do Tauri converte para camelCase. A confirmação de
      // identidade do mesmo link não passa por aqui: ela ficou no Rust, que é
      // quem confere.
      joinSecret: token ?? null,
    });


    // Daqui em diante quem manda é `#tela-auth`: o veredito da chave é lido
    // numa tela antes de se entrar em sala de voz nenhuma, e a entrada mudou de
    // lugar junto com ele. Enquanto isso não acontece, a sessão continua
    // desenhada com o que o `connect` já devolveu — quem chegar nela não espera
    // o primeiro tique do laço de snapshot.
    desenhar(snapshot);
    entrarNaAutenticacao(snapshot, veredito, alvo, nossoServidor);
  } catch (falha) {
    // `connect` responde por `ConnectFailure` desde esta tarefa: o erro de
    // sempre **mais a trilha**. Quem escreve a frase quer o erro; a trilha vai
    // para o console, que é onde alguém que está investigando a procura — «o
    // primeiro deu prazo esgotado em 4 s, o quarto recusou» é o dado que faltou
    // quando o teste de campo das duas casas falhou.
    //
    // O `?? falha` não é zelo: `analisar_convite` e o `AlreadyConnected` deste
    // mesmo comando respondem com o enum cru, e a mesma função lê os dois.
    const motivo = falha?.error ?? falha;
    if (Array.isArray(falha?.trail) && falha.trail.length > 0) {
      console.warn("chegada:", falha.trail);
    }
    // Uma batida que ficou pendente na portaria tem tela própria (ADR 0030), e
    // uma falha que chega enquanto essa tela está na frente pertence a ela:
    // `#boot-erro` estaria escondido atrás. `levarParaAEspera` responde se
    // tratou a falha, e só o que sobra vira a linha vermelha daqui.
    if (!levarParaAEspera(motivo, ultimoAlvo ?? "")) {
      erro.hidden = false;
      // O apelido que esta tentativa mandou, para a recusa por nome tomado
      // poder dizer **qual** nome foi recusado. Ver `fraseDeErro`.
      erro.textContent = fraseDeErro(motivo, apelido);
    }
  } finally {
    botao.disabled = false;
    // A travessia acabou, de um jeito ou de outro. O que sobra na tela é o
    // veredito ou o erro, e não o último candidato tentado.
    mostrarEtapa(null);
  }
}

/**
 * Escreve onde a chegada está, ou apaga a linha.
 *
 * `null` esconde: uma linha vazia e visível empurra o formulário para baixo a
 * cada tentativa, e um `role="status"` vazio é anunciado como nada.
 */
function mostrarEtapa(frase) {
  const linha = $("boot-etapa");
  if (frase === null) {
    linha.hidden = true;
    linha.textContent = "";
    return;
  }
  // Visível primeiro, escrita depois: um `role="status"` escondido no instante
  // em que o texto muda não é anunciado. É a mesma ordem da faixa de veredito.
  linha.hidden = false;
  linha.textContent = frase;
}

// As etapas da chegada, enquanto ela acontece.
//
// Chegam pelo mesmo canal de sempre, e são o único evento da lista que existe
// **antes** de haver sessão: a FFI recebe a ponte antes de bloquear no aperto
// de mão (`Connection::connect_watching`). Sem isto o spinner desta tela era mudo, e
// quando o teste de campo das duas casas falhou ninguém soube dizer em que
// ponto — quatro candidatos eram tentados em série atrás dos três blocos.
listen("seele://event", (evento) => {
  const payload = evento.payload;
  if (!payload || typeof payload !== "object" || !payload.ConnectStageChanged) return;
  // Só com a tela de entrada na frente. Uma reconexão futura que publicasse
  // etapas escreveria numa tela escondida atrás da sessão.
  if ($("tela-boot").hidden) return;
  mostrarEtapa(fraseDeEtapa(payload.ConnectStageChanged.stage));
});

/**
 * Diz até onde o link recém-criado chega, embaixo dele.
 *
 * O `alcance` é o degrau da escada do ADR 0022 em que o servidor parou, como
 * nome estável — `PortaNoRoteador`, `FuroDeNat`, `Ipv6Direto`, `RedeLocalOuVpn`
 * ou `SoRedeLocal` —, e a frase mora no `FRASES`, que é onde moram todas.
 *
 * Só os degraus que **não** alcançam de fora ganham destaque, e são os únicos
 * que precisam: os outros são boas notícias, e uma boa notícia gritada vira
 * ruído que se aprende a ignorar — inclusive no dia em que a notícia for ruim.
 *
 * `RedeLocalOuVpn` conta como perto: quem hospeda com uma VPN de navegação
 * ligada tem um endereço que parece alcançar o mundo e não aceita ninguém.
 */
function mostrarAlcance(alcance, portaRecusada, encontroRecusado, firewallNaoCobre) {
  const onde = $("convite-alcance");
  const frase = fraseDeErro(alcance);
  const soPerto = alcance === "SoRedeLocal" || alcance === "RedeLocalOuVpn";

  onde.textContent = frase;
  onde.classList.toggle("convite-alcance-curto", soPerto);
  onde.classList.toggle("convite-alcance-longe", !soPerto);

  // Por que cada degrau de cima não deu, quando chegou a ser tentado. Vai
  // embaixo e menor: é a pista de quem for investigar, não a mensagem de quem
  // só quer mandar o link. São duas linhas e não uma porque numa casa em que
  // nem o roteador abriu a porta nem o ponto de encontro respondeu são **duas**
  // informações, e quem for investigar precisa das duas.
  //
  // **Sem rótulo colado na frente**, e isto foi conserto: cada frase já se
  // nomeia — as quatro do roteador dizem «roteador» e as do encontro dizem
  // «ponto de encontro». Um `o roteador respondeu:` aqui produziu, numa tela de
  // verdade, «o roteador respondeu: o roteador respondeu, e o endereço dele…».
  // O rótulo só parecia necessário porque quem o escreveu não estava lendo a
  // frase que ele ia prefixar.
  // **A parede vem antes, e não é detalhe.**
  //
  // Os dois motivos abaixo explicam por que o link não chega tão longe quanto
  // poderia. Este explica por que ele não chega a lugar nenhum: a regra de
  // firewall desta máquina nomeia um programa que não é o que está escutando,
  // então nem quem está na mesma rede entra.
  //
  // Medido numa máquina de verdade: o anfitrião subia anunciando furo de NAT, e
  // quem estava na mesma LAN batia três vezes sem entrar. A escada dizia até
  // onde o link ia; nada dizia que a porta estava fechada atrás dela. O relato
  // foi «teste em LAN não funciona».
  if (firewallNaoCobre) {
    const parede = document.createElement("span");
    parede.className = "convite-alcance-parede";
    parede.textContent = firewallNaoCobre;
    onde.append(parede);
  }

  for (const motivo of [portaRecusada, encontroRecusado]) {
    if (!motivo) continue;
    const detalhe = document.createElement("span");
    detalhe.className = "convite-alcance-detalhe";
    detalhe.textContent = motivo;
    onde.append(detalhe);
  }

  onde.hidden = false;
}

/**
 * Vira anfitrião: sobe o servidor dentro deste app e entra nele.
 *
 * Duas etapas de propósito. `hospedar` põe o servidor de pé e devolve o link;
 * conectar é o caminho de sempre, com o endereço que ele devolveu. Um servidor
 * hospedado aqui e um do outro lado do mundo entram pela mesma porta.
 */
async function hospedar() {
  const botao = $("botao-hospedar");
  const erro = $("boot-erro");
  botao.disabled = true;
  erro.hidden = true;

  try {
    const anfitriao = await invoke("hospedar");
    // Hospedar aqui é entrar aqui: o endereço da própria máquina vira o
    // alvo da conexão que vem em seguida, e é o que `conectar()` sem argumento
    // vai usar.
    ultimoAlvo = anfitriao.aqui;
    // E entrar aqui não pede para conferir a própria chave — ver `subindoServidorAqui`.
    subindoServidorAqui = true;
    mostrarAlcance(
      anfitriao.alcance,
      anfitriao.porta_recusada,
      anfitriao.encontro_recusado,
      anfitriao.firewall_nao_cobre,
    );

    // **O link, guardado e mostrado.** A comp da 0.9.0 promove a um diálogo o
    // que era um bloco entre outros nesta tela: quem acabou de hospedar precisa
    // de uma coisa só — o link — e precisa dela agora. Um bloco é uma coisa que
    // se acha; um diálogo é uma coisa que chega.
    //
    // Guardado antes de mostrado porque a configuração é onde ele mora depois,
    // e o diálogo promete isso por escrito. Um diálogo que prometesse um lugar
    // vazio seria pior que não prometer nada.
    guardarOLinkDaPorta(anfitriao.convite);
    abrirPorta($("convite-alcance").textContent);

    await conectar();
  } catch (falha) {
    erro.hidden = false;
    erro.textContent = fraseDeErro(falha);
  } finally {
    botao.disabled = false;
  }
}

// ------------------------------------------------------------------- ligação

// `paste` dispara antes de o valor entrar no campo; o tique seguinte já o tem.

// Digitar outro endereço à mão desfaz o convite. `lerConvite` escreve neste
// campo por código, e atribuição não dispara `input` — só o teclado chega aqui.

$("botao-hospedar").addEventListener("click", hospedar);

// **Aqui havia um `desenharVisitados()` de um `desenharVisitados` que não
// existe mais**, e ele foi o defeito mais caro desta leva: uma chamada solta no
// topo de um script comum estoura na carga, e tudo depois dela deixa de ser
// registrado — inclusive o `click` do `CONECTAR`, que fica na tela e não
// responde a nada. A lista de visitados é do diálogo `ONDE VOCÊ JÁ ESTEVE`
// agora, e ele a desenha quando abre.

// ------------------------------------------------- o microfone, antes de entrar
//
// Uma consulta ao sistema, no arranque e a cada volta para esta tela. Não pede
// nada e não muda nada: só lê o que o Windows já decidiu. Ver
// `seele_audio::device::consentimento_do_microfone`.
//
// Aqui e não na sessão porque é aqui que dá para agir a tempo: quem descobre
// que está mudo lá dentro já falou por cinco minutos para uma sala calada. Foi
// exatamente o que aconteceu num teste de campo, e a única coisa que a tela
// daquela pessoa dizia era «SEM ÁUDIO».

/* `conferirMicrofone` saiu da entrada.
 *
 * Ela conferia o microfone antes de conectar, numa tela que a comp da 0.9.0
 * dissolveu. O que ela fazia mora em CONFIGURAÇÕES · MICROFONE E SOM, com o
 * medidor de entrada e a lista de aparelhos — alcançável da entrada pela
 * engrenagem, e também de dentro de uma conversa, que é quando se descobre que
 * a máquina abriu o microfone errado. */

// ----------------------------------------------- as duas saídas da entrada
//
// `CONECTAR` abre onde se escolhe o servidor; `HOSPEDAR AQUI` faz desta máquina
// o servidor. A comp da 0.9.0 põe as duas lado a lado e nada entre elas: a
// entrada deixou de ser um formulário a preencher e virou uma pergunta de duas
// respostas.

$("botao-conectar").addEventListener("click", () => {
  abrirServidores().catch((falha) => console.warn("abrir servidores:", falha));
});

/** Desenha o perfil no rodapé da entrada: a inicial e o nome desta máquina. */
async function desenharPerfilDaEntrada() {
  let apelido = "";
  try {
    apelido = (await invoke("apelido_local")) ?? "";
  } catch (falha) {
    console.warn("apelido_local:", falha);
  }
  // Sem nome escolhido, o travessão — e não um exemplo. Um nome de exemplo no
  // rodapé é um nome que a pessoa acha que já é o dela.
  $("boot-perfil-nome").textContent = apelido || "—";

  // E o retrato, no mesmo quadrado. Ele é desta máquina e não do servidor —
  // ver `meu_retrato` no `main.rs` —, então a entrada mostra o mesmo rosto que
  // a sessão vai mostrar. Um rodapé que só sabe a inicial enquanto o diálogo
  // que ele abre mostra a foto é o mesmo desencontro que este trabalho
  // consertou um nível acima.
  const quadrado = $("boot-perfil-inicial");
  quadrado.textContent = (apelido || "?").trim().charAt(0).toUpperCase();
  let retrato = null;
  try {
    retrato = await invoke("meu_retrato");
  } catch (falha) {
    console.warn("meu_retrato:", falha);
  }
  if (retrato) {
    quadrado.style.backgroundImage = `url(${retrato})`;
    quadrado.dataset.comRetrato = "sim";
    quadrado.textContent = "";
  } else {
    quadrado.style.removeProperty("background-image");
    delete quadrado.dataset.comRetrato;
  }
}

$("boot-perfil").addEventListener("click", () => {
  abrirPerfil()
    .then(desenharPerfilDaEntrada)
    .catch((falha) => console.warn("abrir o perfil:", falha));
});

desenharPerfilDaEntrada().catch((falha) => console.warn("perfil da entrada:", falha));
