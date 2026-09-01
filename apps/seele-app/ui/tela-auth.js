// SEELE — a entrada no servidor (`#tela-auth`).
//
// Duas entradas e dois trabalhos, escolhidos por `data-modo` na `<section>`.
//
// `aperto` — chega pelo `connect` bem-sucedido de `tela-boot.js` e sai para
// `#tela-sessao`. É o aperto de mão virando etapa: antes dela, o único vestígio
// dele na janela era o `CONEXÃO SEGURA` no cabeçalho da sessão, escrito depois
// de tudo já ter acontecido, inclusive de já se estar dentro de uma sala de voz.
//
// `espera` — chega pelo `connect` **derrubado** com `AdmissionPending`, a
// portaria do ADR 0030. Nasceu de um teste de verdade: um amigo bateu, leu a
// frase certa numa linha de erro da tela de entrada, e ficou sem saber o que
// fazer com ela. Aqui a mesma frase tem tela, e a tela responde as três
// perguntas de quem espera — o que aconteceu, o que fazer agora, e o que não
// adianta fazer.
//
// ---- um movimento, e não três ----
//
// Esta tela pedia três apertos no MESMO botão para uma coisa só: um para
// conferir a identidade, outro para entrar, e — na primeira vez de toda pessoa
// nova, que é justamente quando o servidor ainda não a liberou — um terceiro,
// repetido quantas vezes fossem precisas até acertar o instante em que quem
// hospeda decidiu. Quem usou descreveu assim: «fica clicando repetidas vezes
// num mesmo lugar».
//
// Agora é um. A conferência não sumiu — ela é o que impede alguém no meio do
// caminho, continua acontecendo dentro do `connect` e continua escrita no B·02
// —, o que ela deixou de ser é um pedágio que se paga com o dedo. O que ela
// ganhou em troca foi voz: o botão diz que está conferindo enquanto confere,
// porque um botão que fica mudo trabalhando é um botão que se aperta de novo.
//
// ---- e a espera passou a esperar sozinha ----
//
// Aqui estava escrito, em negrito, que **nada tenta de novo sozinho**: um
// relógio batendo na porta de outra pessoa seria o «nada espera» do ADR 0030
// virando espera com outro nome, e ainda gastaria os baldes de fichas do
// ADR 0025.
//
// A metade do argumento que era sobre o servidor continua de pé, e é ela que
// escolhe o intervalo lá embaixo. A metade que era sobre a pessoa não estava:
// a recusa por falta de permissão é a **primeira** resposta que toda pessoa
// nova recebe, e o preço de não ter relógio era ela ficar apertando um botão às
// cegas até acertar o momento. Trocar o dedo dela por um intervalo escolhido
// contra o balde não bate mais na porta — bate menos, e em ritmo constante.
//
// O que o ADR 0030 decide continua intacto: a conexão cai no mesmo instante,
// nada fica pendurado do lado do servidor, e a decisão é durável. O que mudou é
// de quem é o dedo, e por quanto tempo: enquanto esta tela estiver aberta.
// Fechou, parou — e o pedido continua guardado lá, que é o ponto do ADR.
//
// ---- o que esta tela sabe, e o que ela deixou de desenhar ----
//
// O `connect` devolve duas coisas: o snapshot e o veredito da chave. As duas são
// fato apurado, e são o que os painéis mostram.
//
// Quatro campos do comp v2 saíram, e a saída é a correção de uma convenção
// aplicada longe demais. Eles eram desenhados com a moldura de pé e o valor
// escrito como ausente — a contagem de operadores do servidor, a rota, o codec e a
// chave de identidade local —, e nenhum deles existe: os dois primeiros não são
// conceito em lugar nenhum do protocolo, o codec só ganha valor depois de o
// entrar numa sala de voz e nesta tela ninguém entrou ainda, e a chave local não
// atravessa a FFI. Mostrar a falta serve a uma lacuna que se pretende fechar;
// nenhuma destas se fechava, e sete campos com quatro travessões não ensinam
// nada — dão uma tela que parece quebrada. A lacuna fica registrada aqui e no
// `index.html`, que é onde lacuna se registra.
//
// O carimbo de hora das linhas do B·02 é o relógio local no instante em que
// **esta janela** observou o fato. Não é um tempo que veio do servidor, e não
// pretende ser: é estado da casca, da mesma natureza do relógio do cabeçalho.
// Em `espera` é ele que guarda o histórico das tentativas, que é a única coisa
// que essa tela tem para contar e ninguém mais conta.

"use strict";

/** O que o `connect` devolveu, entre a chegada nesta tela e a saída dela. */
let aperto = null;

/**
 * De quantos em quantos segundos a espera bate de novo.
 *
 * O número sai do balde de fichas do servidor, e não do gosto de ninguém.
 * `crates/seele-server/src/taxa.rs` guarda trinta apertos de mão de rajada por
 * endereço de origem, repostos a trinta por minuto — meio aperto por segundo,
 * sustentados, para **tudo** que vier daquele endereço. Quinze segundos são
 * quatro batidas por minuto: um oitavo da reposição, o que deixa sete pessoas
 * esperando atrás do mesmo endereço ainda dentro do orçamento, e sem comer as
 * tentativas de reconexão de quem já está dentro.
 *
 * Quinze também não é número inventado aqui: é o teto da espera exponencial da
 * bateria de reconexão (`crates/seele-core/src/battery.rs`, `MAX_BACKOFF`).
 * Este mesmo servidor já recebe batidas nesse ritmo de todo cliente que caiu da
 * rede, e ninguém chamou isso de inundação. Esta tela não bate mais forte do
 * que a reconexão que já existe ao lado dela.
 *
 * Mais rápido é inundar o servidor de outra pessoa, e é exatamente o que o
 * balde freia — quem é freado fica de fora **por tempo**, por ter insistido, e
 * a tela que o freou teria produzido o dano que ela existe para evitar. Mais
 * devagar e a espera parece travada: meio minuto olhando para uma tela parada é
 * indistinguível de um app morto, e a pessoa volta a procurar botão, que é o
 * defeito que isto existe para tirar. A contagem regressiva sob o botão é a
 * outra metade dessa escolha — quinze segundos que se veem passar não são
 * espera, são andamento.
 */
const SEGUNDOS_ENTRE_BATIDAS = 15;

/**
 * As três frases do estado da conexão.
 *
 * `Snapshot.link_state` já é exatamente `Offline` / `Unverified` / `Verified`, decidido no
 * core — a casca não classifica nada aqui, só escolhe como dizer. A chave
 * continua sendo a do protocolo; o que mudou é a palavra que sai na tela.
 *
 * Os três rótulos dizem a mesma coisa que os `PADRÃO: …` do comp diziam, sem
 * pedir que se aprenda uma cor antes: o que interessa a quem lê é se a
 * identidade do outro lado foi conferida.
 */
const PADROES = {
  Offline: {
    rotulo: "SEM CONEXÃO",
    nota: "Sem sessão aberta. Nada a verificar até o servidor responder.",
  },
  Unverified: {
    rotulo: "CONEXÃO NÃO VERIFICADA",
    nota: "A identidade deste servidor ainda não foi conferida.",
  },
  Verified: {
    rotulo: "CONEXÃO SEGURA",
    nota: "Identidade confirmada. Captura liberada.",
  },
};

/** O travessão duplo: a moldura está desenhada e o valor não existe. */
const AUSENTE = "——";

/**
 * Quantas linhas o B·02 guarda antes de esquecer as mais velhas.
 *
 * A lista era curta por construção enquanto cada linha custava um aperto de
 * botão. Agora a espera escreve duas linhas por batida, sozinha, enquanto esta
 * janela estiver aberta — uma tarde inteira são milhares de nós num painel que
 * ninguém está lendo, e o topo dele é o instante mais inútil da espera.
 *
 * Duzentas são cerca de vinte e cinco minutos de espera no intervalo escolhido
 * acima: mais do que alguém rola para trás, e pouco o bastante para o painel
 * não crescer sem fim.
 */
const LINHAS_DO_REGISTRO = 200;

/**
 * Entra na tela de autenticação com o que o `connect` acabou de devolver.
 *
 * Chamada de `tela-boot.js`, de dentro do manipulador do formulário — que é
 * sempre depois de os sete arquivos terem rodado, e é o que permite a uma tela
 * chamar a outra sem `import` (ADR 0019).
 *
 * Também chamada **de dentro desta tela**, quando uma batida da espera enfim
 * passou. Esse caso não para para pedir um clique, e a diferença é `liberado`:
 * quem esperava já apertou o botão que valia, lá atrás, e foi cuidar da vida
 * enquanto o dono decidia. Pedir a ele mais um aperto seria devolver o pedágio
 * na única entrada em que, por construção, ninguém está olhando para a tela.
 */
function entrarNaAutenticacao(snapshot, veredito, endereco) {
  const tela = $("tela-auth");
  const liberado = !tela.hidden && tela.dataset.modo === "espera";
  esperando = false;
  pararDeBater();

  aperto = { snapshot, veredito };

  // Só faz sentido vindo da entrada. Numa liberação o foco já está nesta tela,
  // e não há foco da tela de entrada para guardar.
  if (!liberado) guardarFoco("tela-boot");
  $("tela-boot").hidden = true;
  $("tela-auth").hidden = false;
  // De volta ao aperto de mão: a admissão saiu, e a tela que a esperava não tem
  // mais assunto.
  tela.dataset.modo = "aperto";
  // O alvo do foco volta a ser o botão. Em espera ele é a saída, porque lá o
  // botão está desabilitado e um alvo que não aceita foco cai na `<section>`.
  tela.dataset.foco = "auth-botao";
  $("auth-parede").hidden = true;

  $("auth-endereco").textContent = endereco || AUSENTE;
  // A sessão precisa do mesmo endereço para a porta do cabeçalho, e este é o
  // último ponto do caminho que ainda o tem: o `Snapshot` não o carrega.
  guardarAlvoDoServer(endereco);
  desenharPadrao(snapshot);
  desenharServerDaEntrada(snapshot);
  dizerSeOConviteJaConferiu(veredito);

  // Um botão, um movimento. Conferir e entrar eram dois passos deste mesmo
  // botão, e a pessoa pagava os dois para fazer uma coisa só.
  const botao = $("auth-botao");
  botao.textContent = "ENTRAR NO SERVIDOR";
  botao.disabled = false;

  if (liberado) {
    // O registro da espera **não** é limpo aqui: ele é o histórico das
    // tentativas, e a última linha dele é a boa notícia.
    registrar("PERMISSÃO CONCEDIDA — ENTRANDO NO SERVIDOR", "azul");
    // Dito em voz alta antes de a sessão se anunciar: quem esperava não estava
    // olhando, e a notícia é a liberação, não a tela que vem depois dela.
    anunciar("Permissão concedida. Entrando no servidor.");
    entrarNoServidor().catch((falha) => console.warn("entrada:", falha));
    return;
  }

  // Uma linha, e ela é verdade: este endereço foi resolvido, agora, por esta
  // janela. As outras quatro do `AUTH_LOG_BASE` do comp descrevem etapas de um
  // aperto de mão que o core faz inteiro do lado de lá, sem relatar nenhuma.
  repovoar($("auth-registro"), []);
  registrar(`RESOLVENDO ${endereco || AUSENTE}`, "apagado");

  // No fim, e não junto do `hidden`: o alvo é o `auth-botao`, e ele acabou de
  // ser reescrito e reabilitado duas dúzias de linhas acima.
  abrirTela("tela-auth");
}

/**
 * A cartela do estado da conexão, a nota que a acompanha, e o botão que a veste.
 *
 * O kanji ao lado do rótulo saiu junto com o resto do japonês decorativo. O
 * elemento continua na marcação e é esvaziado aqui: uma cartela que guardasse o
 * 青 da entrada anterior ficaria com o único caractere que já não é dito.
 */
function desenharPadrao(snapshot) {
  const padrao = PADROES[snapshot?.link_state] ?? PADROES.Offline;
  const cartela = $("auth-padrao");
  cartela.dataset.padrao = snapshot?.link_state ?? "Offline";
  $("auth-padrao-rotulo").textContent = padrao.rotulo;
  $("auth-padrao-kanji").textContent = "";
  $("auth-padrao-nota").textContent = padrao.nota;
  $("auth-botao").dataset.padrao = snapshot?.link_state ?? "Offline";
}

/**
 * Diz, antes de qualquer clique, que o convite já conferiu a identidade.
 *
 * Achado ALTO da avaliação de usabilidade, e o mesmo terreno do botão de dois
 * passos: quem colou um link do ADR 0006 **já** teve a chave conferida contra a
 * impressão digital que o link carrega, em Rust, dentro do `connect`. A tela
 * pedia mesmo assim um `VERIFICAR IDENTIDADE` — o mesmo botão, com o mesmo
 * texto de quem chega sem convite nenhum — e a conferência que ele anunciava já
 * tinha acontecido. Pedir de novo o que já foi feito ensina que o pedido não
 * significa nada.
 *
 * Agora a conferência é dita onde ela pode ser lida antes de se decidir
 * qualquer coisa, e o que sobra na tela é a entrada, direta.
 *
 * `null` esconde a linha em vez de deixá-la vazia: fora do convite verificado
 * não há nada a afirmar aqui, e uma moldura vazia afirmaria que há.
 */
function dizerSeOConviteJaConferiu(veredito) {
  const conferido = veredito?.FirstContactVerified ?? null;
  const linha = $("auth-conferido");
  linha.hidden = conferido === null;
  linha.textContent = conferido
    ? `O CONVITE JÁ CONFIRMOU A CHAVE DESTE SERVIDOR\n${conferido.fingerprint}`
    : "";
}

/**
 * A ficha do servidor — C·02.
 *
 * Três valores, e os três saem do snapshot: o nome do servidor, quantas salas
 * de voz e quantos canais de texto. O travessão que sobra é o do carregamento,
 * e não o de um valor que não existe: um snapshot sem lista é um snapshot que
 * ainda não chegou.
 *
 * O codec estava aqui e saiu. Ele lia `telemetry.bitrate_bps`, que só ganha
 * valor depois de se entrar numa sala de voz — e nesta tela ninguém entrou ainda,
 * então ele saía `——` em toda entrada que já aconteceu. Um campo que é sempre
 * travessão é um campo, e não uma lacuna: a telemetria de verdade está na barra
 * permanente da sessão, que é onde ela tem o que medir.
 */
function desenharServerDaEntrada(snapshot) {
  $("auth-server-nome").textContent = snapshot?.server || AUSENTE;
  $("auth-voice_rooms").textContent = doisDigitos(snapshot?.voice_rooms?.length);
  $("auth-linhas").textContent = doisDigitos(snapshot?.channels?.length);
}

/** `03`, como o comp escreve — e `——` quando não há lista para contar. */
function doisDigitos(quantos) {
  return typeof quantos === "number" ? String(quantos).padStart(2, "0") : AUSENTE;
}

/**
 * Uma linha no B·02, com o relógio local do instante em que ela foi observada.
 *
 * `tom` escolhe a cor, e nunca é o que carrega a informação: o texto da linha
 * diz o que aconteceu por escrito (`specs/06-clientes-gui.md`).
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
  const lista = $("auth-registro");
  lista.append(linha);
  // As mais velhas saem por cima: numa espera longa o que interessa é o fim da
  // lista, e o começo dela é o instante em que ainda não havia o que contar.
  while (lista.childElementCount > LINHAS_DO_REGISTRO) lista.firstElementChild.remove();
}

/**
 * Conferir e entrar, num aperto só.
 *
 * Eram dois passos do mesmo botão, e a ordem entre eles nunca foi uma escolha
 * de quem usa — não existe entrar sem conferir, e conferir sem entrar é uma
 * tela que não leva a lugar nenhum. Duas metades obrigatórias de um movimento
 * não são duas decisões; são uma decisão cobrada duas vezes.
 *
 * O botão é desabilitado durante as duas e reescrito no fim: se a entrada
 * falhar, o que fica na tela é um botão que volta a dizer o que faz, e não um
 * rótulo de meio caminho.
 */
async function entrarNoServidor() {
  const botao = $("auth-botao");
  botao.disabled = true;
  try {
    await verificarIdentidade();
    await inserirPlug();
  } finally {
    botao.textContent = "ENTRAR NO SERVIDOR";
    botao.disabled = false;
  }
}

/**
 * A primeira metade do movimento: dizer o que o aperto de mão decidiu.
 *
 * Nenhuma comparação acontece aqui — ela já aconteceu, em Rust, dentro do
 * `connect` (ADR 0003), e o que chegou foi o resultado mais a impressão digital
 * para uma pessoa conferir por outro canal. O que mudou é que este resultado
 * deixou de ser cobrado como um clique à parte: ele é escrito no caminho.
 *
 * O rótulo do botão vira o que está acontecendo. Não é enfeite: entre o aperto
 * e a sessão há uma ida ao core, e um botão que emudece durante uma espera é um
 * botão que se aperta de novo.
 */
async function verificarIdentidade() {
  const botao = $("auth-botao");
  botao.textContent = "CONFERINDO IDENTIDADE…";

  const frase = fraseDoVeredito(aperto?.veredito);
  if (frase) {
    registrar(frase, "atencao");
  } else {
    // `Known` não vira frase de propósito (`tela-sessao.js`): repetir "a chave é
    // a mesma de sempre" a cada entrada é ensinar a não ler a linha no dia em
    // que ela não for. O que se diz é que a conferência aconteceu.
    registrar("CHAVE CONFERIDA — NADA MUDOU DESDE A ÚLTIMA ENTRADA");
  }

  // O estado da conexão pode ter mudado entre o `connect` e agora; o valor
  // volta a ser lido em vez de deduzido. `atualizar()` não serve porque desenha a sessão.
  try {
    const snapshot = await invoke("snapshot");
    aperto.snapshot = snapshot;
    desenharPadrao(snapshot);
    desenharServerDaEntrada(snapshot);
    registrar(PADROES[snapshot.link_state]?.rotulo ?? PADROES.Offline.rotulo, tomDoPadrao(snapshot));
  } catch (falha) {
    console.warn("snapshot:", falha);
  }
}

function tomDoPadrao(snapshot) {
  if (snapshot.link_state === "Verified") return "azul";
  if (snapshot.link_state === "Unverified") return "atencao";
  return "apagado";
}

/**
 * A segunda metade do movimento: a sessão começa.
 *
 * **O canal de texto abre sozinho; a sala de voz não.** As duas coisas já foram
 * automáticas
 * juntas, com um motivo bom — chegar numa tela vazia é chegar sem saber o que
 * fazer. O motivo continua bom para uma delas e nunca foi bom para a outra.
 *
 * Texto é passivo: ninguém te ouve por você estar lendo, então abrir o primeiro
 * canal resolve a tela vazia sem te comprometer com nada.
 *
 * Entrar numa sala de voz não é passivo. É ocupar uma das quinze vagas,
 * aparecer na lista como presente, e pôr um microfone à disposição de uma
 * conversa que você não escolheu. Quem chegou num servidor para ler o que
 * perdeu acordava dentro de
 * uma sala de voz sem ter apertado nada — foi o relato de quem usou: «não dá
 * para você ficar fora de uma sala».
 *
 * As duas coisas eram uma só aqui e são duas no modelo mental de quem usa.
 * Agora são duas aqui também.
 *
 * É o que torna seguro entrar sozinho quando a espera é liberada: o que
 * acontece sem clique nenhum é passivo do começo ao fim.
 */
async function inserirPlug() {
  try {
    // **Lido agora, e não o do aperto de mão.** `aperto.snapshot` é o retrato
    // de quando a conexão foi feita, e a lista de Linhas chega do servidor
    // *depois* dele. Quando esta função corria com aquele retrato ainda vazio,
    // nenhuma Linha era aberta — e o servidor **só entrega mensagem a quem
    // entrou no canal** (`session.rs`, `channels.contains`). O resultado em
    // campo foi uma conversa que não chegava, até a pessoa clicar numa Linha
    // para digitar: o clique entra no canal e busca o histórico, e tudo aparecia
    // de uma vez.
    //
    // Ler agora fecha o caso comum. O caso em que a lista ainda não chegou nem
    // agora é fechado do outro lado, em `tela-sessao.js`: ver `abrirLinhaSozinho`.
    let snapshot = aperto?.snapshot;
    try {
      snapshot = await invoke("snapshot");
    } catch (falha) {
      console.warn("snapshot ao entrar:", falha);
    }
    if (snapshot?.channels?.length > 0) {
      await invoke("open_channel", { channel: snapshot.channels[0].id });
    }

    $("tela-auth").hidden = true;
    $("tela-sessao").hidden = false;
    // O veredito continua indo para a faixa da sessão. Ele foi lido aqui por
    // quem estava olhando esta tela; a faixa é onde ele fica disponível para
    // quem chegou depois, e é `role="status"`, que esta lista não é.
    mostrarVeredito(aperto?.veredito ?? null);
    // A imagem do servidor, uma vez, aqui.
    //
    // Ela **não** vem por evento nesta hora, e essa é a razão de esta linha
    // existir: o servidor manda `ServerIconChanged` logo depois do aperto de mão, a
    // ponte dobra a mensagem e emite `ServerChanged` — e ninguém está ouvindo
    // ainda. A casca só se inscreve depois de `Connection::connect` voltar, e a
    // travessia inteira já aconteceu quando ela volta. O único evento que
    // atravessa essa janela é `ConnectStageChanged`, e ele existe justamente
    // porque foi entregue **antes** do bloqueio.
    //
    // O sintoma era um cabeçalho sem distintivo em todo servidor que já tinha
    // imagem escolhida — inclusive o próprio, ao hospedar. Abrir a tela de
    // configuração consertava por acidente, porque lá o ícone é sincronizado
    // no desenho.
    await seguirOServidor();
    // E o que se acabou de aprender fica anotado para a próxima vez: a lista de
    // servidores salvos passa a mostrar o nome e o distintivo em vez de um
    // endereço IP, que é o que ninguém decora.
    invoke("lembrar_aparencia_do_servidor").catch((falha) =>
      console.warn("lembrar_aparencia_do_servidor:", falha),
    );
    await atualizar();
    // Depois do desenho, e sem `data-foco`: a operação recebe o foco na própria
    // `<section>`. Ver a marcação dela — focar o compositor aqui desligaria o
    // push-to-talk no primeiro segundo de sessão.
    abrirTela("tela-sessao");
  } catch (falha) {
    registrar(fraseDeErro(falha), "atencao");
  }
}

// ---------------------------------------------------- a espera pela portaria
//
// ADR 0030. Quem bate num servidor com portaria é derrubado **no instante**, de
// propósito: segurar a conexão obrigaria a um prazo, e um prazo fabrica
// «ninguém atendeu», que é uma resposta sobre a qual não se pode agir. O que o
// desenho não tinha era o outro lado disso — a pessoa caía de volta na tela de
// entrada com uma linha de erro, e uma linha de erro não diz que voltar amanhã
// funciona.
//
// A conexão continua caindo. O que muda é que ela cai numa tela, e que a tela
// bate de novo por conta própria, no intervalo escolhido em
// `SEGUNDOS_ENTRE_BATIDAS`, até a permissão sair ou até esta janela sair da
// frente.

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
 *   Uma batida desta tela que não alcançou o servidor precisa ser lida aqui:
 *   `#boot-erro` está atrás desta tela, e uma mensagem escrita numa tela
 *   escondida é uma mensagem que ninguém recebe. A contagem recomeça, porque
 *   um servidor fora do ar é motivo para insistir e não para desistir.
 *
 * `AdmissionDenied` chegando da tela de entrada **não** vem para cá, e a
 * assimetria é deliberada: uma recusa não é uma espera, não há o que
 * acompanhar, e a tela de entrada é onde se escolhe outro servidor. Chegando aqui,
 * numa batida feita de dentro da espera, ela é o fim desta espera e é dita
 * aqui — mudar de tela para dar uma resposta que a pessoa está esperando seria
 * arrancá-la do lugar em que ela perguntou.
 */
function levarParaAEspera(falha, endereco) {
  const tela = $("tela-auth");
  // Quem saiu pela porta enquanto uma batida estava no ar levou a espera junto.
  // Reabrir esta tela por cima da entrada seria arrancar a pessoa do lugar em
  // que ela acabou de escolher ficar — e a linha vermelha da entrada também não
  // tem o que dizer sobre uma espera abandonada. A falha morre aqui, tratada.
  //
  // A corrida existia antes e era preciso apertar dois botões seguidos para
  // vê-la. Agora a batida parte de um relógio, então ela pode acontecer com a
  // pessoa apertando um botão só, e a culpa deixou de poder ser dela.
  if (tela.hidden && batendoDaEspera) return true;

  const naEspera = !tela.hidden && tela.dataset.modo === "espera";
  const razao = falha?.Refused?.reason;
  if (razao !== "AdmissionPending" && !naEspera) return false;

  const chegando = tela.hidden;
  const frase = fraseDeErro(falha);
  // Não há aperto de mão nenhum: o `connect` não devolveu snapshot nem veredito,
  // e deixar os de uma conexão anterior aqui seria mostrar a ficha de um
  // servidor em que esta pessoa não entrou.
  aperto = null;
  tela.dataset.modo = "espera";
  $("auth-endereco").textContent = endereco || AUSENTE;
  $("auth-espera-frase").textContent = frase;
  $("auth-conferido").hidden = true;

  // O botão continua desenhado — um botão que some é um botão que a pessoa
  // procura — e aqui ele deixou de ser um caminho: é o estado. Quem espera não
  // tem o que apertar, porque esta tela bate por ele; quem foi recusado não tem
  // o que apertar porque a resposta já saiu. Nos dois casos o único caminho vivo
  // é a saída, e é para lá que o foco vai.
  const recusado = razao === "AdmissionDenied";
  const botao = $("auth-botao");
  botao.textContent = recusado ? "ENTRADA NEGADA" : "AGUARDANDO PERMISSÃO";
  // O botão veste o estado da conexão, e aqui não há sessão nenhuma. Sem isto
  // ele guardaria o azul da última que houve — um `CONEXÃO SEGURA` pintado num
  // botão que existe porque a entrada não aconteceu.
  botao.dataset.padrao = "Offline";
  botao.disabled = true;
  tela.dataset.foco = "auth-voltar";
  $("auth-parede").hidden = !recusado;
  $("auth-parede").textContent = recusado
    ? "Tentar de novo dá na mesma. Só quem hospeda pode mudar essa resposta."
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

  // Uma recusa decidida é o fim: continuar contando seria bater numa porta que
  // já respondeu, e é a única batida que o balde do servidor cobra de graça.
  esperando = !recusado;
  if (recusado) pararDeBater();
  else contarParaBater();

  // `abrirTela` só na chegada. Numa batida feita daqui o teclado já está nesta
  // tela, e devolvê-lo arrancaria de quem tivesse tabulado até a saída — a
  // notícia, essa, é anunciada nas duas.
  if (chegando) abrirTela("tela-auth", frase);
  else anunciar(frase);
  return true;
}

/**
 * O tique de um segundo da espera, ou `null` quando nada está contando.
 *
 * Um só relógio para as duas coisas — mostrar quanto falta e bater quando
 * acabar. Dois relógios seriam duas verdades sobre o mesmo instante, e a que a
 * pessoa lê é a que erraria.
 */
let contagem = null;

/** Quantos segundos faltam para a próxima batida. */
let faltam = 0;

/**
 * Se esta tela está esperando ser liberada.
 *
 * Distinta de `contagem !== null`, e a diferença é o ponto: o relógio para
 * quando ninguém está olhando, e a espera **não** acaba por isso. Sem esta
 * bandeira, minimizar a janela seria indistinguível de desistir, e voltar a ela
 * não teria o que retomar.
 */
let esperando = false;

/**
 * Se a batida em curso partiu desta tela, e não do formulário da entrada.
 *
 * Só serve para uma coisa: saber, quando a resposta chega, se a tela que a
 * pediu ainda está na frente.
 */
let batendoDaEspera = false;

/**
 * Para de contar e apaga a contagem da tela.
 *
 * Chamada de todo lugar por onde esta espera acaba — liberação, recusa e saída
 * pela porta —, porque um relógio que sobrevive à tela que o justificava bate
 * na porta de outra pessoa sem ninguém para ler a resposta.
 */
function pararDeBater() {
  if (contagem !== null) clearInterval(contagem);
  contagem = null;
  $("auth-repique").textContent = "";
}

/**
 * Recomeça a contagem para a próxima batida.
 *
 * A contagem é escrita antes do primeiro tique: um campo que nasce vazio e só
 * ganha número um segundo depois pisca a cada tentativa.
 */
function contarParaBater() {
  pararDeBater();
  if (!alguemOlhando()) {
    // Sem ninguém olhando não há batida, e a tela diz por quê em vez de ficar
    // muda: quem volta precisa saber que a espera não morreu, só parou.
    $("auth-repique").textContent = "ESPERA PAUSADA — VOLTE À JANELA";
    return;
  }
  faltam = SEGUNDOS_ENTRE_BATIDAS;
  escreverContagem();
  contagem = setInterval(() => {
    faltam -= 1;
    if (faltam > 0) {
      escreverContagem();
      return;
    }
    // O relógio morre antes da batida, e quem a responde é que decide se ele
    // volta: `levarParaAEspera` recomeça a contagem, `entrarNaAutenticacao` não.
    // Sem isto, uma batida lenta se sobreporia à seguinte.
    pararDeBater();
    $("auth-repique").textContent = "TENTANDO ENTRAR…";
    baterDeNovo().catch((falha) => console.warn("espera:", falha));
  }, 1000);
}

function escreverContagem() {
  $("auth-repique").textContent = `NOVA TENTATIVA EM ${faltam} s`;
}

/**
 * Se esta janela está sendo olhada por alguém.
 *
 * A condição inteira desta espera automática, e o motivo é o ADR 0030. Ele
 * recusou segurar a conexão enquanto quem hospeda decide, e listou três razões;
 * a terceira é literalmente **«o caso que importa, o da janela minimizada»**.
 *
 * Bater de tempo em tempo não é o que aquele ADR recusou — cada batida conecta,
 * é recusada e desconecta, sem segurar recurso nenhum do outro lado. Mas uma
 * janela minimizada batendo na porta de um estranho a cada quinze segundos, por
 * horas, com ninguém na frente da tela para ver a resposta, é a versão de
 * cliente do mesmo defeito: gasto no servidor de outra pessoa por uma espera que
 * ninguém está esperando.
 *
 * Então a espera acompanha o olho. Quem está olhando entra sem apertar nada;
 * quem minimizou para de bater e volta a bater ao voltar.
 */
function alguemOlhando() {
  return document.visibilityState === "visible";
}

/**
 * Bate de novo, uma vez.
 *
 * Reaproveita o `conectar` da tela de entrada inteiro — mesmo endereço, mesmo
 * apelido, mesmo convite —, e é ele quem decide para onde isto vai: admitido,
 * `entrarNaAutenticacao` reescreve esta tela como aperto de mão e segue direto
 * para a sessão; ainda não, `levarParaAEspera` a reescreve como espera e
 * recomeça a contagem.
 *
 * Uma batida por vez: quem a chama já parou o relógio, e só as duas saídas
 * acima o religam.
 */
async function baterDeNovo() {
  registrar("BATENDO DE NOVO", "apagado");
  batendoDaEspera = true;
  try {
    await conectar();
  } finally {
    batendoDaEspera = false;
  }
}

/**
 * Sai da espera pela porta por onde se entrou.
 *
 * Existe só em `espera`, e existe porque ali não há sessão: quem espera pode
 * querer outro servidor, outro apelido, ou só fechar o assunto. No aperto de mão
 * não há esta saída, e não deve haver — lá a sessão está aberta, e o caminho de
 * volta é encerrá-la.
 *
 * Sair para de bater. A espera é desta tela, e não do app: o pedido continua
 * guardado no servidor, que é o que o ADR 0030 garante.
 */
function voltarParaAEntrada() {
  // A bandeira antes do relógio: sair pela porta encerra a espera, e não a
  // pausa. Sem isto, voltar à janela depois de desistir recomeçaria a bater.
  esperando = false;
  pararDeBater();
  guardarFoco("tela-auth");
  $("tela-auth").hidden = true;
  $("tela-boot").hidden = false;
  voltarParaTela("tela-boot");
  esvaziarBarraDoServidor();
}

// ------------------------------------------------------------------- ligação

$("auth-botao").addEventListener("click", entrarNoServidor);

$("auth-voltar").addEventListener("click", voltarParaAEntrada);

/*
 * A espera segue o olho.
 *
 * Sair da janela para a contagem; voltar a ela recomeça. `esperando` e não
 * `#auth-espera` escondido: a mesma tela serve à entrada e à espera, e só a
 * segunda tem o que retomar.
 */
document.addEventListener("visibilitychange", () => {
  if (!esperando) return;
  if (alguemOlhando()) contarParaBater();
  else pararDeBater();
});
