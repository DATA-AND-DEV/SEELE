// O aceite dos MODs de um servidor: a tela que faltava — ADR 0045.
//
// # O que existia antes deste arquivo
//
// Tudo, menos a tela. O servidor anunciava a lista, o `seele-core` a recusava
// com `ConnectionError::ModsNaoAceitos`, e os três verbos de gravar o sim
// (`aceite_de_mods`, `aceitar_mods`, `esquecer_aceite_de_mods`) estavam
// registrados no `main.rs` — **chamados por ninguém**. O que a pessoa lia era
// uma frase no `frases.js` terminando em «A tela para ler e aceitar esta lista
// ainda não existe neste app.»
//
// Na prática: **um servidor com MOD habilitado não tinha como ser entrado pelo
// aplicativo.** O motor inteiro estava pronto e o produto não tinha a porta.
//
// # O que esta tela mostra, e o que ela recusa mostrar
//
// Ela mostra o que o produto **sabe**: identificador, versão, hash do conteúdo,
// repositório público, o que o MOD declara alcançar, e se ele roda na máquina
// de quem hospeda. Tudo isso vem do anúncio, que é o que o ADR 0045 manda pôr
// diante da pessoa antes de qualquer byte ser baixado.
//
// **Ela não mostra selo de procedência.** O protótipo de interface desenha
// «OFICIAL · ASSINADO» e «VERIFICADO», e eles não entram: nada neste produto
// hoje verifica assinatura de MOD — o indexador não está no ar. Um selo que o
// código não sustenta é pior que selo nenhum, porque ele é exatamente a coisa
// em que a pessoa se apoiaria para dizer sim sem ler o resto.
//
// # O que o sim significa
//
// Ele vale para **esta combinação** de servidor e conjunto de MODs. Trocou um
// MOD, trocou uma versão, entrou um novo: a identidade do conjunto muda e a
// pergunta volta. Quem grava isso é o Rust; esta tela só passa adiante a
// identidade que veio no anúncio, inteira e como chegou.

"use strict";

/** Para onde o teclado volta quando o diálogo fecha. */
let focoAntesDoAceite = null;

/** O que este diálogo está perguntando agora. */
let aceitePendente = null;

/**
 * Desenha uma linha da lista.
 *
 * O alcance vem em etiquetas separadas, e não numa frase corrida, porque é o
 * que a pessoa compara entre um MOD e outro. «roda na máquina de quem hospeda»
 * fica **fora** da lista de alcances, numa linha própria: ela não é mais um
 * alcance entre outros — é código de terceiro rodando na máquina de outra
 * pessoa, e o ADR 0045 a separa pelo mesmo motivo.
 */
function linhaDeMod(mod, indice) {
  const linha = elemento("li", "mods-item");

  const ordem = elemento("span", "mods-ordem", String(indice + 1).padStart(2, "0"));
  const corpo = elemento("div", "mods-corpo");

  corpo.append(
    elemento("div", "mods-id", mod.id),
    elemento("div", "mods-versao", `versão ${mod.version}`),
  );

  const repo = elemento("div", "mods-repo");
  repo.append(elemento("span", "rotulo", "REPOSITÓRIO"), elemento("span", "", mod.repo || "não declarado"));
  corpo.append(repo);

  // O hash é o que prova que os bytes são os que foram revisados — ADR 0026,
  // «TLS diz de que servidor o arquivo veio, não quem o produziu». Cortado na
  // tela porque 64 caracteres empurram tudo, e inteiro no `title` para quem
  // precisa comparar.
  const hash = elemento("div", "mods-hash");
  const curto = (mod.hash || "").slice(0, 16);
  hash.append(elemento("span", "rotulo", "CONTEÚDO"), elemento("span", "", curto ? `${curto}…` : "sem hash"));
  hash.title = mod.hash || "";
  corpo.append(hash);

  const alcances = elemento("div", "mods-alcances");
  if (Array.isArray(mod.reach) && mod.reach.length > 0) {
    for (const alcance of mod.reach) {
      alcances.append(elemento("span", "mods-etiqueta", alcance));
    }
  } else {
    alcances.append(elemento("span", "mods-etiqueta mods-etiqueta-vazia", "nada declarado"));
  }
  corpo.append(alcances);

  if (mod.no_servidor) {
    corpo.append(
      elemento(
        "p",
        "mods-servidor",
        "Roda na máquina de quem hospeda: rede de saída, relógio e registro.",
      ),
    );
  }

  linha.append(ordem, corpo);
  return linha;
}

/**
 * Abre a pergunta.
 *
 * @param {string} alvo      o servidor, como a pessoa o digitou
 * @param {Array}  mods      o que veio no anúncio
 * @param {string} conjunto  a identidade do conjunto, para gravar o sim
 */
async function abrirAceiteDeMods(alvo, mods, conjunto) {
  aceitePendente = { alvo, conjunto, mods };
  focoAntesDoAceite = document.activeElement;

  // **Já houve um sim aqui, para outra lista?**
  //
  // É a pergunta que separa «nunca estive neste servidor» de «este servidor
  // trocou de MOD desde a última vez», e as duas pedem atenção diferente da
  // pessoa. Sem ela, quem entra num servidor conhecido lê a mesma tela da
  // primeira visita e não tem como saber que algo mudou — que é exatamente o
  // momento em que ler a lista importa mais.
  let anterior = null;
  try {
    anterior = await invoke("aceite_de_mods", { alvo });
  } catch (falha) {
    // Não saber se houve um sim antes não impede de perguntar agora. O aviso
    // some e o resto da tela vale.
    console.warn("aceite anterior:", falha);
  }
  const mudou = Boolean(anterior) && anterior !== conjunto;
  $("mods-mudou").hidden = !mudou;

  const camada = $("mods-aceite");
  $("mods-contagem").textContent =
    mods.length === 1 ? "1 MOD" : `${mods.length} MODS`;
  $("mods-alvo").textContent = alvo;
  repovoar($("mods-lista"), mods.map(linhaDeMod));
  $("mods-erro").hidden = true;
  $("mods-aceitar").textContent =
    mods.length === 1 ? "ACEITAR 1 MOD E ENTRAR" : `ACEITAR ${mods.length} MODS E ENTRAR`;
  $("mods-aceitar").disabled = false;

  camada.hidden = false;
  // O foco vai para **recusar**, e não para aceitar. Um Enter distraído numa
  // pergunta sobre rodar código de terceiro não pode ser um sim.
  $("mods-recusar").focus();
}

/** Fecha e devolve o teclado a quem o tinha. */
function fecharAceiteDeMods() {
  $("mods-aceite").hidden = true;
  aceitePendente = null;
  if (focoAntesDoAceite && document.contains(focoAntesDoAceite)) {
    focoAntesDoAceite.focus();
  }
  focoAntesDoAceite = null;
}

/**
 * Grava o sim, **obtém o conjunto autorizado**, e tenta entrar de novo.
 *
 * **Por que a obtenção mora aqui, e não no laço que carrega os MODs.**
 *
 * Até a v0.11.0 aceitar gravava o sim e reconectava, e mais nada. O que era
 * exigido e não estava nesta máquina virava uma anotação — `sem-pacote` — que
 * só aparecia dentro da tela de MODs. O relato de campo foi «entrei como
 * convidado num servidor com MOD de estilo e a cor não mudou»: o pacote nunca
 * chegou, e nada na sessão disse isso a quem estava olhando.
 *
 * Obter aqui, e não no `carregarMods`, é o que mantém a promessa do aceite:
 * ele autoriza **aquele conjunto exato**, o que está escrito na tela e nada
 * mais. Um conjunto que mude depois é outra decisão e pede outro sim — por
 * isso o laço periódico continua sem baixar nada por conta própria.
 */
async function aceitarOsMods() {
  if (!aceitePendente) return;
  const { alvo, conjunto, mods } = aceitePendente;
  const botao = $("mods-aceitar");
  const erro = $("mods-erro");
  botao.disabled = true;
  erro.hidden = true;
  try {
    await invoke("aceitar_mods", { alvo, conjunto });
  } catch (falha) {
    // O sim não foi gravado. **Não tenta entrar**: entrar agora falharia de
    // novo pelo mesmo motivo, e a pessoa leria a mesma pergunta duas vezes sem
    // saber que o problema foi gravar.
    botao.disabled = false;
    erro.hidden = false;
    erro.textContent = fraseDeErro(falha);
    return;
  }

  if (!(await obterOConjunto(mods ?? []))) {
    // **Não entra.** Entrar sem os pacotes é chegar à sessão sem o que o
    // servidor exige e descobrir depois, pela cor que não mudou. A tela fica
    // aberta com o motivo, e os dois caminhos que o documento pede — tentar de
    // novo, ou sair — continuam à mão: o botão volta, e recusar fecha.
    botao.disabled = false;
    botao.textContent = "TENTAR DE NOVO";
    return;
  }

  fecharAceiteDeMods();
  // A mesma porta de sempre. O aceite já está em disco, então esta tentativa
  // passa pelo ponto que recusou a anterior.
  await conectar();
}

/**
 * Põe nesta máquina o conjunto exato que acabou de ser autorizado.
 *
 * Devolve `true` quando tudo o que o servidor exige está no disco com o hash
 * exigido. Uma cópia local só vale se **bater o hash**: aceitar não é confiar
 * em qualquer versão que leve o mesmo nome.
 *
 * @param {Array} mods o anúncio, como a tela o mostrou
 */
async function obterOConjunto(mods) {
  const erro = $("mods-erro");
  const dizer = (frase) => {
    erro.hidden = false;
    erro.textContent = frase;
  };

  let instalados;
  try {
    instalados = await invoke("mods_instalados");
  } catch (falha) {
    dizer(`não consegui ler o que está instalado aqui: ${fraseDeErro(falha)}`);
    return false;
  }

  const temOExato = (m) =>
    instalados.some((i) => i.id === m.id && i.hash === m.hash);
  const faltando = mods.filter((m) => !temOExato(m));
  if (faltando.length === 0) return true;

  for (const [quantos, m] of faltando.entries()) {
    dizer(`obtendo ${m.id} (${quantos + 1} de ${faltando.length})…`);
    try {
      await invoke("instalar_mod_do_catalogo", { id: m.id, versao: m.version });
    } catch (falha) {
      // **Nomeia o MOD, a etapa e o próximo passo.** «Falhou» sozinho manda a
      // pessoa adivinhar qual dos três, e o que fazer a respeito.
      dizer(
        `não consegui obter ${m.id} ${m.version}: ${fraseDeErro(falha)}. ` +
          "Tente de novo, ou recuse a entrada.",
      );
      return false;
    }
  }

  // **Conferir depois de obter, e não confiar no sucesso da obtenção.**
  // O catálogo pode ter servido outra coisa sob a mesma versão; o que vale é o
  // hash que o servidor exige. Sem esta volta, entrar aqui seria entrar com o
  // que o catálogo quis dar, e não com o que foi autorizado.
  dizer("verificando…");
  try {
    instalados = await invoke("mods_instalados");
  } catch (falha) {
    dizer(`não consegui conferir o que chegou: ${fraseDeErro(falha)}`);
    return false;
  }
  const errados = mods.filter((m) => !temOExato(m));
  if (errados.length > 0) {
    dizer(
      `o que chegou não é o que o servidor exige: ${errados
        .map((m) => m.id)
        .join(", ")}. Não vou entrar com outro conteúdo.`,
    );
    return false;
  }
  erro.hidden = true;
  return true;
}

/**
 * Se esta falha é a pergunta dos MODs, abre a tela e diz que tratou.
 *
 * O mesmo formato de `levarParaAEspera`: quem chama não precisa saber o que
 * aconteceu, só se ainda tem uma linha vermelha para escrever.
 */
function levarParaOAceiteDeMods(motivo, alvo) {
  const pedido = motivo?.ModsNaoAceitos;
  if (!pedido) return false;
  abrirAceiteDeMods(alvo, pedido.mods ?? [], pedido.conjunto ?? "").catch((falha) =>
    console.warn("abrir aceite:", falha),
  );
  return true;
}

// ------------------------------------------------------------------- ligação

$("mods-aceitar").addEventListener("click", () => {
  aceitarOsMods().catch((falha) => console.warn("aceitar mods:", falha));
});
$("mods-recusar").addEventListener("click", fecharAceiteDeMods);

// Apertar fora recusa, como em toda camada deste app. É o gesto que todo mundo
// tenta primeiro, e aqui ele cai no lado seguro: sair da pergunta não entra em
// lugar nenhum e não grava sim nenhum.
fecharAoClicarFora("mods-aceite", fecharAceiteDeMods);

// Escape recusa, como em toda camada deste app. Recusar é o lado seguro: sair
// da pergunta não entra em lugar nenhum.
$("mods-aceite").addEventListener("keydown", (evento) => {
  if (evento.key === "Escape") {
    evento.preventDefault();
    fecharAceiteDeMods();
  }
});

// --------------------------------------------- a gestão, em CONFIGURAÇÕES
//
// O par da camada de cima. Aquela é de quem **entra** e responde à pergunta;
// esta é de quem **hospeda** e decide o que a sala exige — mais a metade que
// vale para todo mundo: desfazer um sim já dado.
//
// Os três verbos existiam desde a entrega do ADR 0045, registrados no `main.rs`
// e chamados por ninguém. Sem esta tela, habilitar um MOD só era possível
// editando o banco à mão.

/** Desenha uma linha da lista de instalados. */
function linhaDeModInstalado(mod, hospedando) {
  const linha = elemento("li");
  const caixa = elemento("div", "server-dispositivo mods-linha-gestao");

  const texto = elemento("span", "server-dispositivo-nome");
  texto.append(elemento("span", "mods-id", mod.id));

  // **Um MOD recusado aparece, e aparece dizendo por quê.** Escondê-lo faria a
  // pessoa que copiou a pasta procurar um MOD que o produto viu e descartou em
  // silêncio — «o produto sabe e não conta».
  if (mod.refused) {
    texto.append(
      elemento("span", "mods-recusado", `não serve: ${mod.refused}`),
    );
    caixa.append(texto);
    linha.append(caixa);
    return linha;
  }

  texto.append(
    elemento("span", "mods-versao", `versão ${mod.version} · revisão ${String(mod.hash || "").slice(0, 8)}`),
  );

  // **O estado, em quatro palavras que não se confundem** — U11.
  //
  // «Após ativar os MODs, permanece uma mensagem "instalado, e desligado".» A
  // frase era escrita uma vez, ao instalar, e ficava na tela enquanto a pessoa
  // ligava o MOD ao lado dela. A auditoria pediu «mensagens derivadas do estado
  // atual, distinguir instalado/selecionado/ativo/em execução».
  //
  // As quatro são quatro perguntas diferentes, e antes duas palavras
  // respondiam as quatro:
  //
  // - **instalado**: os bytes estão nesta máquina;
  // - **selecionado**: o rascunho desta tela o inclui, e o SALVAR ainda não foi;
  // - **ativo**: o servidor o exige agora;
  // - **em execução**: o código dele subiu nesta janela — e isso é `fase`,
  //   logo abaixo, porque só o carregador sabe.
  //
  // Derivado a cada desenho, e não guardado: um estado guardado é o que fica
  // para trás quando o de verdade muda.
  const remover = elemento("button", "botao-fantasma", "APAGAR MOD");
  remover.type = "button";
  remover.disabled = mod.enabled || modsExigidos.get(mod.id) === mod.hash;
  remover.title = remover.disabled ? "Desligue o MOD e salve antes de apagar." : "Apagar o pacote desta máquina";
  remover.addEventListener("click", () => abrirConfirmacao(
    "APAGAR MOD?", `Apagar ${mod.id} desta máquina? Os dados dos servidores serão preservados.`,
    "APAGAR MOD", async () => {
      try {
        await invoke("apagar_pacote_do_cache", { hash: mod.hash });
        rascunho.delete(chaveDoPacote(mod));
        await desenharMods();
      } catch (falha) {
        $("mods-gestao-erro").textContent = fraseDeErro(falha);
        $("mods-gestao-erro").hidden = false;
      }
    },
  ));
  caixa.append(remover);
  const chave = chaveDoPacote(mod);
  const selecionado = rascunho.has(chave);
  const ativo = conjuntoNoServidor.has(chave);
  texto.append(elemento(
    "span",
    "mods-estado",
    ativo && selecionado ? "ATIVO NESTE SERVIDOR"
      : selecionado ? "SELECIONADO · falta SALVAR"
        : ativo ? "SERÁ DESLIGADO · falta SALVAR"
          : "INSTALADO E DESLIGADO",
  ));
  // **O que aconteceu com ele, por fase** — A06 da auditoria.
  //
  // A gestão dizia instalado e ligado, e nada mais. Tudo o que podia dar errado
  // — pacote ausente, versão diferente da exigida, catálogo sem resposta,
  // script que não carregou — morria no console, e num aplicativo empacotado o
  // console não é lugar nenhum. Quem usava via o MOD na lista e nenhum botão
  // dele na tela, sem próximo passo.
  const estado = modsExigidos.get(mod.id) === mod.hash ? estadoDosMods.get(mod.id) : null;
  if (estado) {
    const frase = FASES_DO_MOD[estado.fase];
    if (frase) {
      // Só o que deu certo fica com a cor de nota; o resto é recusa, e recusa
      // tem cor própria nesta casca.
      const tranquilo = ["carregado", "carregando"].includes(estado.fase);
      const classe = tranquilo ? "mods-versao" : "mods-recusado";
      texto.append(
        elemento("span", classe, estado.detalhe ? `${frase} — ${estado.detalhe}` : frase),
      );
    }
  }
  if (mod.server) {
    texto.append(
      elemento("span", "mods-servidor", "roda na máquina de quem hospeda"),
    );
  }
  caixa.append(texto);

  // **Para quem entrou, não há interruptor — há um fato.**
  //
  // Aqui ficava um botão DESLIGAR desabilitado, e um botão morto é uma
  // pergunta sem resposta: ele sugere que desligar seria possível noutro
  // momento, e desligar um MOD que o servidor exige **não** é uma decisão que
  // caiba a quem entrou. O conjunto foi acordado na porta; sair dele sem sair
  // do servidor seria estar na sessão fingindo cumprir o que não cumpre.
  //
  // Quem quer sair do conjunto sai do servidor, e isso a porta já oferece.
  if (!hospedando && modsExigidos.has(mod.id)) {
    caixa.append(elemento("span", "mods-exigido", "EXIGIDO POR ESTE SERVIDOR"));
    linha.append(caixa);
    return linha;
  }

  // **O interruptor mexe no rascunho, e não no servidor.**
  //
  // Antes cada clique gravava na hora, e cada gravação acorda o anúncio — que
  // derruba quem está dentro. Relatado assim: «cada ativação expulsa o host da
  // sessão e exige nova entrada; ativar vários MODs repete o processo para
  // cada um». Agora o servidor só muda no SALVAR, e muda de uma vez.
  const ligadoNoRascunho = rascunho.has(chaveDoPacote(mod));
  const botao = elemento("button", "botao-fantasma");
  botao.type = "button";
  botao.textContent = ligadoNoRascunho ? "DESLIGAR" : "LIGAR";
  // Sem hospedar não há servidor em que ligar. Desabilitado **e** explicado
  // logo abaixo da lista: um botão morto sem motivo é uma pergunta sem resposta.
  botao.disabled = !hospedando;
  // O que está pendente é dito na própria linha, e não só no rodapé: quem rola
  // uma lista longa não vê o rodapé enquanto decide.
  if (hospedando && ligadoNoRascunho !== mod.enabled) {
    botao.classList.add("mods-pendente");
    texto.append(
      elemento(
        "span",
        "mods-versao",
        ligadoNoRascunho ? "vai passar a ser exigido" : "vai deixar de ser exigido",
      ),
    );
  }
  botao.addEventListener("click", () => {
    if (ligadoNoRascunho) rascunho.delete(chaveDoPacote(mod));
    else {
      // **Um pacote por MOD.** Ligar outro conteúdo do mesmo MOD substitui o
      // anterior no rascunho: um servidor não exige duas versões do mesmo MOD,
      // e deixar as duas marcadas diria que exige.
      for (const chave of [...rascunho]) {
        if (chave.split("\u0000")[0] === mod.id) rascunho.delete(chave);
      }
      rascunho.add(chaveDoPacote(mod));
    }
    desenharMods().catch((falha) => console.warn("desenhar mods:", falha));
  });
  caixa.append(botao);

  linha.append(caixa);
  return linha;
}

/**
 * A linha de um MOD que este servidor exige e que não está nesta máquina.
 *
 * Sem interruptor de propósito, pela mesma razão escrita em
 * `linhaDeModInstalado`: o conjunto foi acordado na porta, e quem entrou não
 * decide sobre ele. O que esta linha faz é o que faltava — **dizer** que ele é
 * exigido, dizer qual conteúdo, e dizer em que pé está.
 *
 * Ela aparece no alto da lista, antes do que está instalado: é o que está
 * faltando, e é o que a pessoa veio resolver.
 */
function linhaDeModQueFalta(id, hash) {
  const linha = elemento("li");
  const caixa = elemento("div", "server-dispositivo mods-linha-gestao");
  const texto = elemento("span", "server-dispositivo-nome");
  texto.append(elemento("span", "mods-id", id));

  const estado = estadoDosMods.get(id);
  const frase = FASES_DO_MOD[estado?.fase];
  texto.append(
    elemento(
      "span",
      "mods-recusado",
      frase && estado.detalhe
        ? `${frase} — ${estado.detalhe}`
        : (frase ??
          `o servidor exige o conteúdo ${String(hash).slice(0, 16)}…`),
    ),
  );

  caixa.append(texto);
  caixa.append(elemento("span", "mods-exigido", "EXIGIDO POR ESTE SERVIDOR"));
  linha.append(caixa);
  return linha;
}

/**
 * O que cada fase quer dizer para quem lê — A06.
 *
 * Identificador de um lado, frase do outro, como todo o resto desta casca: a
 * fase é escrita pelo carregador em `base.js`, e uma fase sem frase aqui
 * simplesmente não é desenhada, em vez de mostrar o identificador cru.
 *
 * «Carregado» é sobre os **bytes**, e a frase diz isso: se o MOD estourou
 * dentro da própria inicialização, o navegador avisa que carregou do mesmo
 * jeito. Quem sabe se ele terminou de subir é ele.
 */
const FASES_DO_MOD = {
  carregando: "buscando o código deste MOD…",
  carregado: "código carregado nesta janela",
  "nao-carregou": "o código não carregou — reconecte para tentar de novo",
  // **Carregou, e uma volta dele falhou.** Reconectar não conserta isto: o
  // código subiu. Quem precisa agir é quem escreveu o MOD, e a frase manda
  // olhar onde a razão está escrita — o erro vem ao lado desta linha.
  "falhou-rodando": "o código carregou e uma volta dele falhou; veja a razão ao lado",
  obtendo: "buscando este MOD no catálogo…",
  "nao-obtive": "não deu para buscar este MOD; reconecte para tentar de novo",
  "sem-pacote": "o servidor exige este MOD e ele não está instalado aqui",
  "outra-versao": "o que está instalado aqui não é o que o servidor exige",
  descarregado: "descarregado: o servidor deixou de exigi-lo",
};

// ------------------------------------------------- o rascunho do conjunto
//
// **Por que existe.** Cada interruptor gravava no servidor na hora, e cada
// gravação acorda o anúncio, que encerra as sessões cujo conjunto mudou. Ligar
// três MODs derrubava o operador da própria sessão três vezes, e com ele todo
// mundo que estava lá. O documento de ajustes de 18/09 pede «no máximo um
// ciclo de reconexão por aplicação, não um por switch».
//
// O rascunho é a seleção pendente. O servidor continua com o conjunto dele até
// o SALVAR, que aplica tudo num ato — `aplicar_conjunto_de_mods`, que escreve
// numa transação e acorda o anúncio uma vez só.

// ------------------------------------------- a apresentação, quando há disputa

/**
 * Quem apresenta cada ponto da interface, quando mais de um MOD o substitui.
 *
 * # Por que esta escolha existe e é visível
 *
 * A API 4 permite a um MOD **substituir** a apresentação de uma pessoa, de um
 * canal ou da aparência do servidor. Dois MODs que peçam o mesmo ponto são uma
 * disputa, e o §6 do plano diz como ela não pode ser resolvida: «Nada de
 * "última resposta assíncrona vence".»
 *
 * Sem escolha registrada, a prioridade declarada decide — determinística e
 * estável. Com escolha, ela ganha: uma pessoa decidiu, e um número não.
 *
 * # Por que ela mora nesta máquina
 *
 * Porque a alternativa seria o servidor decidir a apresentação de quem entra
 * nele, e isso é uma decisão sobre a tela de outra pessoa. `localStorage` e
 * não o servidor, por destino: um MOD preferido aqui não vale no próximo
 * servidor, porque não é o mesmo conjunto de MODs.
 */
const PREFERENCIA_DE_APRESENTACAO = "seele.mods.apresentacao";

/**
 * A chave de uma preferência: **destino e ponto**.
 *
 * O registro de decisões descrevia isto como «por destino» e a chave era
 * global por ponto — a revisão de 20/09/2026 apontou a divergência. Ela
 * importa: escolher o PERFIS para apresentar pessoas num servidor não é
 * escolhê-lo em todos, porque nem todos têm o PERFIS instalado, e um deles
 * pode ter outro MOD no lugar.
 *
 * `alvoDaSessao()` mora em `tela-sessao.js` e é o identificador do destino.
 * Fora de sessão não há destino, e aí a chave é a global — o que preserva o
 * que já estava gravado e dá um lugar para a escolha de quem abre a gestão da
 * tela de entrada.
 */
function chaveDaPreferencia(ponto) {
  const alvo = typeof alvoDaSessao === "function" ? alvoDaSessao() : null;
  return `${alvo || "-"}\u0000${String(ponto)}`;
}

/** O mapa de preferências desta máquina, `{ destino + ponto: escolha }`. */
function preferenciasDeApresentacao() {
  try {
    const lido = JSON.parse(localStorage.getItem(PREFERENCIA_DE_APRESENTACAO) ?? "{}");
    return lido && typeof lido === "object" && !Array.isArray(lido) ? lido : {};
  } catch {
    // Armazenamento bloqueado, cheio ou com lixo dentro. Sem preferência é um
    // estado correto — a prioridade decide —, e não um erro a mostrar.
    return {};
  }
}

/**
 * Qual MOD apresenta este ponto, por escolha de quem usa esta máquina.
 *
 * Chamada de `tela-sessao.js` a cada linha do roster, então ela é lida do
 * cache e não do armazenamento: `localStorage.getItem` é síncrono e bloqueia o
 * quadro, e uma faixa com vinte pessoas o chamaria vinte vezes por retrato.
 *
 * Devolve `""` (automático), `":nativo"` (o SEELE desenha) ou o `id` de um MOD.
 * Os três são estados diferentes; ver `escolherSubstituicao`.
 */
let preferenciasLidas = null;
function modPreferidoPara(ponto) {
  preferenciasLidas ??= preferenciasDeApresentacao();
  const escolhido = preferenciasLidas[chaveDaPreferencia(ponto)];
  return typeof escolhido === "string" ? escolhido : "";
}

/** O cache é por destino: trocar de servidor o invalida. */
function esquecerPreferenciasLidas() {
  preferenciasLidas = null;
}

/**
 * Escolhe quem apresenta um ponto: um MOD, o nativo, ou o automático.
 *
 * **Três valores, e não dois** — R5 da revisão de 20/09/2026. «"Usar
 * apresentação padrão" volta à seleção automática de MODs, não ao cartão
 * nativo.» Apagar a preferência devolvia à prioridade, que é justamente o que
 * a pessoa acabou de recusar.
 *
 * @param {string} ponto O ponto de contribuição.
 * @param {string} id O `id` de um MOD, `":nativo"` para o desenho do SEELE, ou
 *   `""` para voltar ao automático.
 */
function escolherApresentacao(ponto, id) {
  preferenciasLidas = { ...preferenciasDeApresentacao() };
  const chave = chaveDaPreferencia(ponto);
  if (id) preferenciasLidas[chave] = String(id);
  else delete preferenciasLidas[chave];
  try {
    localStorage.setItem(PREFERENCIA_DE_APRESENTACAO, JSON.stringify(preferenciasLidas));
  } catch (falha) {
    // Dito, e não engolido: a escolha vale nesta sessão e não sobrevive ao
    // fechamento, e quem escolheu tem direito de saber disso.
    console.warn("preferência de apresentação não foi guardada:", falha);
  }
  // O registro redesenha o que depende dela; ver `contribuicoesDosMods.avisar`.
  if (typeof redesenharAsPessoas === "function") redesenharAsPessoas();
  desenharApresentacoes();
}

/**
 * A seção que mostra quem apresenta o quê — e as disputas.
 *
 * Sem ela, «Usar apresentação padrão» seria um botão para um problema que a
 * pessoa não tem como ver. O §6 pede os dois juntos: a escolha **e** a
 * visibilidade da disputa.
 */
function desenharApresentacoes() {
  const lista = $("lista-apresentacoes");
  const secao = $("mods-apresentacao");
  if (!lista || !secao) return;
  const preferidos = new Map(Object.entries(preferenciasDeApresentacao()));
  const linhas = contribuicoesDosMods.resumo(preferidos);
  secao.hidden = linhas.length === 0;
  if (!linhas.length) {
    lista.replaceChildren();
    return;
  }

  repovoar(lista, linhas.map((linha) => {
    const item = elemento("li", "server-dispositivo mods-linha-gestao");
    const texto = elemento("div", "mods-linha-texto");
    // **O estado, em palavra, e os três são diferentes** — R5.
    //
    // «Apresentado por ninguém» dizia duas coisas ao mesmo tempo: «ninguém
    // substitui este ponto» e «você desligou a substituição». A pessoa que
    // apertou «usar apresentação padrão» não tinha como saber se tinha
    // funcionado.
    const estado = linha.ausente
      ? `você escolheu ${linha.ausente}, e ele não está de pé agora — o SEELE desenha`
      : linha.nativa
        ? "o SEELE desenha; nenhum MOD substitui este lugar"
        : linha.escolhido
          ? `apresentado por ${linha.escolhido}${linha.automatica ? " (escolha automática)" : ""}`
          : `${linha.quantas} contribuição(ões) de ${linha.mods.join(", ")}`;
    texto.append(
      elemento("span", "mods-id", NOMES_DOS_PONTOS[linha.ponto] ?? linha.ponto),
      elemento("span", "mods-versao", estado),
    );
    // **A disputa é dita, e não resolvida em silêncio.** Um MOD preterido que
    // some sem explicação é a pessoa achando que o MOD não funciona.
    if (linha.preteridos.length && !linha.nativa) {
      texto.append(elemento(
        "p",
        "nota",
        `Também pediram este lugar e não o receberam: ${linha.preteridos.join(", ")}. `
        + "Escolha abaixo qual deve desenhar.",
      ));
    }
    item.append(texto);

    const botoes = elemento("div", "mods-linha-botoes");
    for (const candidato of linha.mods) {
      if (candidato === linha.escolhido) continue;
      const usar = elemento("button", "botao-fantasma", `USAR ${candidato}`);
      usar.type = "button";
      usar.addEventListener("click", () => escolherApresentacao(linha.ponto, candidato));
      botoes.append(usar);
    }
    // **Voltar ao nativo é uma escolha, e não a ausência de uma.** Ela some
    // quando já se está nele — um botão que não muda nada é uma pergunta sem
    // resposta.
    if (!linha.nativa) {
      const padrao = elemento("button", "botao-fantasma", "USAR APRESENTAÇÃO DO SEELE");
      padrao.type = "button";
      padrao.addEventListener("click", () => escolherApresentacao(linha.ponto, APRESENTACAO_NATIVA));
      botoes.append(padrao);
    }
    // E desfazer a escolha, voltando ao automático. Só aparece quando há uma:
    // «voltar ao automático» estando no automático não faz nada.
    if (!linha.automatica) {
      const auto = elemento("button", "botao-fantasma", "DECIDIR AUTOMATICAMENTE");
      auto.type = "button";
      auto.addEventListener("click", () => escolherApresentacao(linha.ponto, ""));
      botoes.append(auto);
    }
    item.append(botoes);
    return item;
  }));
}

/**
 * O valor que quer dizer «o SEELE desenha, e nenhum MOD».
 *
 * Espelho de `NATIVO` em `mods-contribuicoes.js`. As duas cópias existem
 * porque a casca não importa módulos, e `a_apresentacao_nativa_tem_um_valor_so`
 * em `tests/frontend.rs` confere que elas não divergem.
 */
const APRESENTACAO_NATIVA = ":nativo";

/** Os pontos ditos em palavra de quem usa, e não no identificador da API. */
const NOMES_DOS_PONTOS = Object.freeze({
  "pessoa.identidade": "Como as pessoas aparecem",
  "pessoa.cartao": "O cartão de cada pessoa",
  "pessoa.detalhes": "O perfil detalhado de uma pessoa",
  "pessoa.acoes": "Ações sobre uma pessoa",
  "canal.item": "Como os canais aparecem na lista",
  "canal.cabecalho": "O cabeçalho de um canal",
  "compositor.ferramentas": "Ferramentas ao escrever",
  "sala.acoes": "Ações dentro de uma sala de voz",
  "servidor.navegacao": "Entradas na navegação",
  "servidor.aparencia": "A aparência da sessão",
});

/**
 * O que estaria ligado se esta tela fosse salva agora, por `id\u0000hash`.
 *
 * **O par, e não o identificador.** Com o cache por conteúdo pode haver dois
 * pacotes do mesmo MOD nesta máquina — um que este servidor exige, outro que
 * outro servidor baixou —, e escolher um deles é a decisão que esta tela toma.
 * Guardar só o identificador diria «exija este MOD» sem dizer quais bytes, que
 * é a ambiguidade que o cache por conteúdo existe para acabar.
 */
const rascunho = new Set();

/** A chave de um pacote no rascunho: qual MOD, e quais bytes. */
function chaveDoPacote(mod) {
  return `${mod.id}\u0000${mod.hash}`;
}

/** O que o servidor exigia quando esta tela leu a lista. */
let conjuntoNoServidor = new Set();

/**
 * A identidade do conjunto sobre o qual este rascunho foi feito.
 *
 * É a base da conferência de conflito: se outro operador aplicou enquanto esta
 * tela estava aberta, gravar por cima apagaria a decisão dele em silêncio.
 */
let baseDoConjunto = "";

/** Impede o segundo envio enquanto o primeiro não voltou. */
let salvando = false;

/** O que muda do conjunto de pé para o rascunho. */
function pendencias() {
  // Os nomes que a pessoa lê são os identificadores; a chave é o par.
  const nome = (chave) => chave.split("\u0000")[0];
  const ligar = [...rascunho].filter((c) => !conjuntoNoServidor.has(c)).map(nome);
  const desligar = [...conjuntoNoServidor].filter((c) => !rascunho.has(c)).map(nome);
  return { ligar, desligar };
}

/** Há edição por salvar? Lido de fora, pelo aviso de saída. */
function haRascunhoPorSalvar() {
  const { ligar, desligar } = pendencias();
  return ligar.length > 0 || desligar.length > 0;
}

/** Desenha a barra do rascunho: o que muda, e os dois botões. */
function desenharRascunho(hospedando) {
  const barra = $("mods-rascunho");
  const { ligar, desligar } = pendencias();
  const mudou = ligar.length > 0 || desligar.length > 0;

  // Sem hospedar não há conjunto a mudar, e a barra não tem o que dizer.
  barra.hidden = !hospedando;
  if (!hospedando) return;

  const frases = [];
  if (ligar.length > 0) frases.push(`passa a exigir: ${ligar.join(", ")}`);
  if (desligar.length > 0) frases.push(`deixa de exigir: ${desligar.join(", ")}`);
  $("mods-pendentes").textContent = mudou
    ? `${frases.join(" · ")}. Nada disso vale até SALVAR.`
    : "Nada pendente: o que está na lista é o que o servidor exige agora.";

  // **Sem diferença, SALVAR não faz nada** — e um botão que não faz nada não
  // pode estar aceso. Salvar sem mudança ainda escreveria, e escrever ainda
  // acorda o anúncio: seria uma queda para todo mundo, por nada.
  $("mods-salvar").disabled = !mudou || salvando;
  $("mods-descartar").disabled = !mudou || salvando;
  $("mods-salvar").textContent = salvando ? "SALVANDO…" : "SALVAR ALTERAÇÕES";
}

/** Devolve o rascunho ao que o servidor exige agora. */
function descartarORascunho() {
  rascunho.clear();
  for (const id of conjuntoNoServidor) rascunho.add(id);
  $("mods-gestao-erro").hidden = true;
  desenharMods().catch((falha) => console.warn("desenhar mods:", falha));
}

/**
 * Aplica o rascunho inteiro, num ato.
 *
 * O que ele **não** faz: um laço sobre `aplicar_mod`. Cada chamada daquele
 * acorda o anúncio, e o ganho inteiro desta tela é que isso aconteça uma vez.
 */
async function salvarAlteracoes() {
  if (salvando || !haRascunhoPorSalvar()) return;
  const { ligar, desligar } = pendencias();
  const erro = $("mods-gestao-erro");
  erro.hidden = true;

  const linhas = [
    "O conjunto que este servidor exige passa a ser outro.",
    ligar.length > 0 ? `Passa a exigir: ${ligar.join(", ")}.` : "",
    desligar.length > 0 ? `Deixa de exigir: ${desligar.join(", ")}.` : "",
    "Quem está dentro agora cai e precisa aceitar o conjunto novo, no aparelho " +
      "de cada um. Uma vez só, e não uma por MOD.",
    "Salvar também registra o seu sim nesta máquina para o conjunto resultante — " +
      "você não vai ser perguntado outra vez pela decisão que acabou de tomar.",
  ].filter(Boolean);

  abrirConfirmacao(
    "APLICAR O CONJUNTO NOVO",
    linhas.join("\n"),
    "SALVAR",
    async () => {
      // **Trancado antes do `await`.** Sem isto, dois cliques rápidos mandam
      // dois conjuntos, e o segundo confere a base contra o que o primeiro
      // acabou de gravar.
      if (salvando) return;
      salvando = true;
      await desenharMods();
      try {
        const escolhidos = [...rascunho].map((chave) => {
          const partes = chave.split("\u0000");
          return { id: partes[0], hash: partes[1] };
        });
        await invoke("aplicar_conjunto_de_mods", {
          ligados: escolhidos,
          base: baseDoConjunto,
        });
      } catch (falha) {
        // **O conjunto mudou por baixo.** O rascunho é preservado: refazê-lo à
        // mão seria perder o trabalho de quem acabou de decidir. A tela
        // recarrega o que está de pé, e as pendências passam a ser contra ele.
        const mudou = falha?.ConjuntoMudou;
        erro.hidden = false;
        erro.textContent = mudou
          ? "outra pessoa mudou o conjunto deste servidor enquanto esta tela " +
            "estava aberta. Nada foi gravado. A lista abaixo já mostra o que " +
            "está valendo agora — confira o que você queria mudar e salve de novo."
          : fraseDeErro(falha);
        salvando = false;
        if (mudou) baseDoConjunto = mudou.atual;
        await desenharMods();
        return;
      }
      salvando = false;
      await desenharMods();
    },
  );
}

/** Desenha as duas listas desta seção. */
/** Mantém uma revisão por MOD à vista, sem esconder as alternativas. */
function linhasDasRevisoes(instalados, hospedando) {
  const abertos = new Set([...$("lista-mods").querySelectorAll("details[open][data-mod]")]
    .map((no) => no.dataset.mod));
  const grupos = new Map();
  for (const mod of instalados) {
    if (!grupos.has(mod.id)) grupos.set(mod.id, []);
    grupos.get(mod.id).push(mod);
  }
  return [...grupos].flatMap(([id, revisoes]) => {
    const prioridade = (mod) => rascunho.has(chaveDoPacote(mod)) ? 2 : mod.enabled ? 1 : 0;
    revisoes.sort((a, b) => prioridade(b) - prioridade(a));
    const principal = linhaDeModInstalado(revisoes[0], hospedando);
    if (revisoes.length === 1) return [principal];
    const item = elemento("li", "mods-revisoes");
    const detalhes = elemento("details", "");
    detalhes.dataset.mod = id;
    detalhes.open = abertos.has(id);
    const titulo = elemento("summary", "", `Outras revisões de ${revisoes[0].name || id} (${revisoes.length - 1})`);
    const lista = elemento("ul", "mods-revisoes-lista");
    lista.append(...revisoes.slice(1).map((mod) => linhaDeModInstalado(mod, hospedando)));
    detalhes.append(titulo, lista);
    item.append(detalhes);
    return [principal, item];
  });
}

async function desenharMods() {
  let instalados = [];
  try {
    instalados = await invoke("mods_instalados");
  } catch (falha) {
    const erro = $("mods-gestao-erro");
    erro.hidden = false;
    erro.textContent = fraseDeErro(falha);
  }

  // **A falha que não é de nenhum MOD** — A06. O carregador guarda sob a chave
  // vazia o caso em que o próprio catálogo do servidor não respondeu: aí não dá
  // para saber o que é exigido, e nenhuma linha da lista pode dizer isso por
  // ele. Sem esta frase, a tela mostraria MODs instalados e quietos, como se
  // estivesse tudo certo.
  const doCatalogo = estadoDosMods.get("");
  if (doCatalogo) {
    const erro = $("mods-gestao-erro");
    erro.hidden = false;
    erro.textContent =
      "não deu para saber quais MODs este servidor exige, então nada foi " +
      "carregado. Reconecte para tentar de novo.";
  }

  // Hospedar é o que decide se LIGAR vale. `mods_instalados` devolve `enabled`
  // falso para todos quando esta janela não hospeda, e não há como distinguir
  // isso de «nenhum ligado» olhando só a lista — daí a pergunta separada.
  let hospedando = false;
  try {
    hospedando = Boolean(await invoke("estou_hospedando"));
  } catch {
    hospedando = false;
  }
  $("mods-sem-hospedar").hidden = hospedando;

  // **De qual servidor esta tela fala.**
  //
  // Antes ela dizia «é do servidor que esta janela hospeda» e parava aí. Com a
  // lista de servidores guardados, quem tem dois não tinha como saber em qual
  // estava mexendo — e o conjunto de MODs é de um servidor, não da máquina.
  const alvo = $("mods-alvo-do-servidor");
  if (hospedando) {
    let qual = null;
    try {
      qual = await invoke("servidor_hospedado");
    } catch (falha) {
      console.warn("servidor hospedado:", falha);
    }
    let nome = null;
    if (qual) {
      try {
        const lista = await invoke("servidores_guardados");
        nome = lista.find((s) => s.id === qual)?.nome ?? null;
      } catch (falha) {
        console.warn("servidores guardados:", falha);
      }
    }
    // Sem identificador é o legado: a máquina que já hospedava antes de haver
    // lista. Dizer «o servidor de sempre» é mais honesto que inventar um nome.
    alvo.textContent = nome
      ? `Estas escolhas valem para: ${nome}`
      : "Estas escolhas valem para o servidor de sempre desta máquina";
    alvo.hidden = false;
  } else {
    alvo.hidden = true;
  }

  // **O conjunto de pé, e o rascunho por cima dele.**
  //
  // O rascunho só é semeado quando não há edição pendente. Refazê-lo a cada
  // desenho — e esta tela redesenha sozinha — apagaria o que a pessoa acabou
  // de marcar, no meio de ela marcar. Se o conjunto mudar por baixo enquanto
  // há rascunho, quem avisa é a conferência de conflito do SALVAR, que diz o
  // que houve em vez de escolher sozinha qual das duas decisões vale.
  const tinhaPendencia = haRascunhoPorSalvar();
  conjuntoNoServidor = new Set(instalados.filter((m) => m.enabled).map(chaveDoPacote));
  if (!tinhaPendencia) {
    rascunho.clear();
    for (const id of conjuntoNoServidor) rascunho.add(id);
    try {
      baseDoConjunto = await invoke("conjunto_exigido_agora");
    } catch (falha) {
      console.warn("conjunto exigido:", falha);
      baseDoConjunto = "";
    }
  }

  // **O que o servidor exige e não está aqui também é uma linha.**
  //
  // A lista era só `mods_instalados`, e o carregador anotava `sem-pacote` para
  // o que faltava. A anotação não tinha onde aparecer: a linha daquele MOD não
  // existia. Quem entrava num servidor que exige um MOD que esta máquina não
  // tem via a lista sem ele e nada mais — o produto sabia e não contava, que é
  // o defeito que o `CLAUDE.md` deste repositório nomeia como o mais caro.
  const faltando = [...modsExigidos]
    .filter(([id]) => !instalados.some((m) => m.id === id))
    .map(([id, hash]) => linhaDeModQueFalta(id, hash));

  const lista = $("lista-mods");
  if (instalados.length === 0 && faltando.length === 0) {
    repovoar(lista, [
      elemento(
        "li",
        "server-dispositivos-vazio",
        "NENHUM MOD INSTALADO NESTA MÁQUINA",
      ),
    ]);
  } else {
    repovoar(lista, [
      ...faltando,
      ...linhasDasRevisoes(instalados, hospedando),
    ]);
  }

  desenharRascunho(hospedando);
  desenharApresentacoes();
  // **A frase da última instalação não sobrevive ao redesenho** — U11.
  //
  // Ela dizia «instalado, e desligado» e ficava na tela enquanto a pessoa
  // ligava o MOD logo abaixo dela: uma afirmação verdadeira no instante em que
  // foi escrita e falsa dois cliques depois. A linha de cada MOD passou a dizer
  // o estado dele, derivado do que é verdade agora, e esta frase volta a ser o
  // que ela é — o resultado de um ato, que dura até a tela se redesenhar.
  if (catalogoJaRedesenhou) $("catalogo-estado").textContent = "";
  catalogoJaRedesenhou = true;
  await desenharOCache();
  await desenharAceites();
}

// ------------------------------------------------------- a manutenção local
//
// Plano de isolamento de 18/09: «gerenciar espaço do cache sem chamar isso de
// configuração de MOD». O cache é endereçado pelo conteúdo, então cada versão
// que passou por aqui deixou uma pasta, e nada nunca a tirava.

// O tamanho é escrito pelo `emBytes` de `frases.js`, e não por um daqui: são
// duas telas mostrando a mesma grandeza, e duas escadas de unidade fariam o
// mesmo arquivo ter dois tamanhos no mesmo produto.

/**
 * A API que este aplicativo oferece, e as que ele executa.
 *
 * Escritas aqui e conferidas contra o Rust por `a_api_da_casca_bate_com_a_do_nucleo`
 * em `tests/frontend.rs`: duas cópias de um número são duas cópias que
 * discordam no dia em que só uma sobe, e esta é usada para dizer a alguém se o
 * problema está no aplicativo ou no pacote.
 */
const API_DESTE_APLICATIVO = 5;
const APIS_QUE_ESTE_APLICATIVO_ACEITA = [5,4,3];

/** Os pacotes guardados nesta máquina, com tamanho e com quem os exige. */
async function desenharOCache() {
  let pacotes = [];
  const erro = $("cache-erro");
  erro.hidden = true;
  try {
    pacotes = await invoke("pacotes_no_cache");
  } catch (falha) {
    erro.hidden = false;
    erro.textContent = fraseDeErro(falha);
    return;
  }

  await desenharOsContadoresDaSessao();

  const total = pacotes.reduce((soma, p) => soma + Number(p.bytes ?? 0), 0);
  $("cache-total").textContent = pacotes.length === 0
    ? "Nenhum pacote guardado."
    : `${pacotes.length} ${pacotes.length === 1 ? "pacote" : "pacotes"}, ${emBytes(total)} no total.`;

  const lista = $("lista-cache");
  if (pacotes.length === 0) {
    repovoar(lista, []);
    return;
  }
  repovoar(
    lista,
    // Do maior para o menor: quem abre esta seção veio por espaço, e o que
    // ocupa mais é o que responde a pergunta que trouxe a pessoa aqui.
    [...pacotes]
      .sort((a, b) => Number(b.bytes ?? 0) - Number(a.bytes ?? 0))
      .map(linhaDePacoteNoCache),
  );
}

/**
 * Os contadores da sessão — etapa E2.
 *
 * Dois números e uma geração. O que eles respondem é a pergunta que uma tela
 * não responde: **sobrou alguma coisa da sessão anterior?** Um evento
 * descartado ou um comando recusado depois de uma saída quer dizer que algo da
 * execução passada continuou falando — e sem o contador isso é invisível,
 * porque o produto já os está recusando em silêncio, que é o certo a fazer com
 * eles e o errado a fazer com a informação.
 *
 * Zeros aparecem, e é de propósito: um contador que só se mostra quando é
 * diferente de zero é um contador que ninguém sabe que existe até o dia em que
 * precisa dele.
 */
async function desenharOsContadoresDaSessao() {
  const linha = $("sessao-contadores");
  if (!linha) return;
  try {
    const estado = await invoke("estado_da_sessao");
    const onde = estado.geracao === 0 ? "fora de sessão" : `sessão nº ${estado.geracao}`;
    // **E o que ainda está de pé do lado da janela.** Os dois números de cima
    // são do Rust e contam o que foi recusado; este é da interface e conta o
    // que **existe** — uma instância `encerrada` com recurso na lista é a forma
    // que «sobrou alguma coisa» tem quando ela acontece.
    const abertos = recursosDePe(modsCarregados);
    const sobra = abertos.length === 0 ? "" : ` · recursos de pé: ${abertos.join(", ")}`;
    // **Uma linha por instância nativa, com a identidade inteira.** Um total
    // não responde «qual delas não está saindo», que é a pergunta de quem abre
    // esta seção depois de desconfiar da máquina.
    const nativos = (estado.mods_nativos ?? [])
      .map(
        (m) =>
          `#${m.instancia} ${m.id} (sessão ${m.geracao}, ${m.hash}…)` +
          (m.encerrando ? " ENCERRANDO" : "") +
          ` · fila ${m.entrada_na_fila}↓/${m.saida_na_fila}↑` +
          (m.saida_recusadas + m.entrada_recusadas > 0
            ? ` · recusadas ${m.entrada_recusadas}↓/${m.saida_recusadas}↑`
            : ""),
      )
      .join(" | ");
    linha.textContent =
      `${onde} · ${estado.eventos_descartados} eventos e ` +
      `${estado.comandos_recusados} comandos recusados por serem de uma sessão encerrada` +
      sobra +
      (nativos ? ` · nativos: ${nativos}` : "");
  } catch (falha) {
    linha.textContent = "";
    console.warn("contadores da sessão:", falha);
  }
}

/** Uma linha da manutenção local. */
function linhaDePacoteNoCache(pacote) {
  const linha = elemento("li");
  const caixa = elemento("div", "server-dispositivo mods-linha-gestao");
  const texto = elemento("span", "server-dispositivo-nome");
  // **O nome, e não o hash** — U10. «MODs antigos aparecem como hashes
  // compridos, `api-too-old`, versão vazia e 0 B. O usuário perde a identidade
  // do pacote justamente quando precisa atualizá-lo.» O identificador vem do
  // manifesto mesmo quando o manifesto foi recusado; só quando nem o JSON abriu
  // é que sobra o nome da pasta, e aí ele é o que há.
  texto.append(elemento("span", "mods-id", pacote.id));
  const medidas = [
    pacote.version ? `versão ${pacote.version}` : "versão não declarada",
    emBytes(pacote.bytes),
    `${String(pacote.hash).slice(0, 12)}…`,
  ];
  texto.append(elemento("span", "mods-versao", medidas.join(" · ")));

  // **A incompatibilidade explicada, com o caminho de saída** — U10.
  //
  // `api-too-old` era tudo o que a tela dizia, e é um código de erro: ele não
  // responde a pergunta que quem lê tem, que é «e agora?». A resposta depende
  // de qual dos dois lados está para trás, e é o número da API que a decide.
  if (pacote.recusado) {
    const nossa = API_DESTE_APLICATIVO;
    const dele = Number(pacote.api) || 0;
    const explicacao = dele === 0
      ? "o manifesto deste pacote não pôde ser lido; ele não roda e não dá "
        + "para saber de que versão ele é"
      : dele > nossa
        ? `este pacote foi feito para a API ${dele}, e este SEELE oferece a `
          + `${nossa}. Atualize o aplicativo em CONFIGURAÇÕES › ATUALIZAÇÃO.`
        : `este pacote foi feito para a API ${dele}, que este SEELE já não `
          + `executa (ele aceita ${APIS_QUE_ESTE_APLICATIVO_ACEITA.join(" e ")}). `
          + "Procure uma versão nova dele no catálogo do servidor.";
    texto.append(elemento("span", "mods-recusado", explicacao));
  }
  caixa.append(texto);

  // **Exigido não ganha botão morto, ganha frase.** Um botão desabilitado
  // sugere que apagar seria possível noutro momento — e seria, depois de
  // desligar o MOD naquele servidor, que é outro ato e mora no bloco de cima.
  if (pacote.exigido_aqui) {
    caixa.append(
      elemento("span", "mods-exigido", "EM USO POR ESTE SERVIDOR"),
    );
    linha.append(caixa);
    return linha;
  }

  const botao = elemento("button", "botao-fantasma");
  botao.type = "button";
  botao.textContent = "APAGAR";
  botao.addEventListener("click", () => {
    apagarUmPacote(pacote.hash).catch((falha) =>
      console.warn("apagar pacote:", falha),
    );
  });
  caixa.append(botao);
  linha.append(caixa);
  return linha;
}

/** Tira um pacote do disco e redesenha as duas listas. */
async function apagarUmPacote(hash) {
  const erro = $("cache-erro");
  erro.hidden = true;
  try {
    await invoke("apagar_pacote_do_cache", { hash });
  } catch (falha) {
    erro.hidden = false;
    erro.textContent = fraseDeErro(falha);
  }
  // As duas: um pacote que sai do cache sai também da lista de instalados.
  await desenharMods();
}

/** A lista de servidores a quem esta máquina já disse sim. */
async function desenharAceites() {
  let aceites = [];
  try {
    aceites = await invoke("aceites_de_mods");
  } catch (falha) {
    console.warn("aceites:", falha);
  }
  const lista = $("lista-aceites");
  if (aceites.length === 0) {
    repovoar(lista, [
      elemento("li", "server-dispositivos-vazio", "VOCÊ AINDA NÃO DISSE SIM A NENHUM"),
    ]);
    return;
  }
  repovoar(
    lista,
    aceites.map((aceite) => {
      const linha = elemento("li");
      const caixa = elemento("div", "server-dispositivo mods-linha-gestao");
      const texto = elemento("span", "server-dispositivo-nome");
      texto.append(elemento("span", "mods-id", aceite.alvo));
      // A identidade do conjunto, cortada: são 64 caracteres, e o que importa
      // na tela é distinguir um sim de outro, não lê-lo inteiro.
      texto.append(
        elemento(
          "span",
          "mods-versao",
          `conjunto ${String(aceite.conjunto).slice(0, 12)}…`,
        ),
      );
      caixa.append(texto);
      const botao = elemento("button", "botao-fantasma");
      botao.type = "button";
      botao.textContent = "DESFAZER";
      botao.addEventListener("click", () => {
        esquecerUmAceite(aceite.alvo).catch((falha) =>
          console.warn("esquecer aceite:", falha),
        );
      });
      caixa.append(botao);
      linha.append(caixa);
      return linha;
    }),
  );
}

/** Desfaz um sim e redesenha. */
async function esquecerUmAceite(alvo) {
  try {
    await invoke("esquecer_aceite_de_mods", { alvo });
  } catch (falha) {
    const erro = $("mods-gestao-erro");
    erro.hidden = false;
    erro.textContent = fraseDeErro(falha);
  }
  await desenharAceites();
}

/** Escolhe uma pasta e instala o que houver nela. */
async function instalarUmMod() {
  const erro = $("mods-gestao-erro");
  const botao = $("mods-instalar");
  erro.hidden = true;
  botao.disabled = true;
  try {
    const id = await invoke("instalar_mod");
    // `null` é a pessoa tendo fechado o diálogo. Não é falha e não merece
    // frase: desistir de escolher é uma resposta.
    if (id) {
      erro.hidden = false;
      erro.classList.add("mods-aviso-bom");
      erro.textContent = `INSTALADO: ${id}. Ele está no disco e desligado — ligue quando quiser que ele valha.`;
    }
  } catch (falha) {
    erro.hidden = false;
    erro.classList.remove("mods-aviso-bom");
    erro.textContent = fraseDeErro(falha);
  } finally {
    botao.disabled = false;
  }
  await desenharMods();
  // **QA-05: o catálogo em mãos também envelheceu.** Instalar por pasta muda o
  // que está nesta máquina, e é disso que a lista do catálogo tira o JÁ
  // INSTALADO e o ATUALIZAR. Sem esta linha, ela continuava marcando como
  // instalada a versão que acabou de ser substituída — e fechar e reabrir as
  // configurações não resolvia, porque o que estava velho era a lista em
  // memória, não a tela.
  if (catalogoEmMaos) await desenharCatalogoEmMaos();
}

$("mods-salvar").addEventListener("click", () => {
  salvarAlteracoes().catch((falha) => console.warn("salvar mods:", falha));
});
$("mods-descartar").addEventListener("click", descartarORascunho);

$("mods-instalar").addEventListener("click", () => {
  instalarUmMod().catch((falha) => console.warn("instalar mod:", falha));
});

// ------------------------------------------------------ o catálogo, no cliente
//
// A outra metade do ADR 0045, e a que não existia: o indexador tinha gerador,
// assinador e site — 167 testes — e o cliente não tinha **uma linha** sobre o
// catálogo. Nem a chave pública de MOD estava embutida.
//
// **Nada é consultado ao abrir a seção.** A busca sai de um botão, pela mesma
// regra do botão de atualizar (ADR 0026): um produto que fala com a rede por ter
// sido aberto é um produto que conta a alguém que ele foi aberto.

/** O que o catálogo devolveu na última busca, para instalar sem buscar de novo. */
let catalogoEmMaos = null;
/**
 * A tela já se redesenhou desde a última instalação?
 *
 * O resultado de uma instalação é dito e dura **um** desenho: o seguinte já
 * tem, na linha do próprio MOD, o estado que é verdade agora.
 */
let catalogoJaRedesenhou = false;

/** O nível de avaliação, escrito como a pessoa lê. */
const NIVEIS = {
  oficial: "OFICIAL",
  verificado: "VERIFICADO",
  "com-notas": "PUBLICADO COM NOTAS",
};

/**
 * As versões da API de MOD que este build aceita. Vem do Rust.
 *
 * Vazio até a resposta chegar, e vazio quer dizer «não sei» — ver
 * `ultimaQueEsteBuildEntende`, que nesse caso não filtra nada em vez de
 * esconder tudo.
 */
let apisDeModAceitas = [];

/**
 * A última versão publicada que **este** build sabe executar.
 *
 * **Era a última da lista, e isso quebrava no dia seguinte a toda subida de
 * API.** O catálogo lista tudo o que já foi publicado; a última entrada pode
 * pedir uma API que este build não tem. Oferecê-la faz quem ainda não atualizou
 * apertar instalar e receber uma recusa — e não lhe dá caminho de volta para a
 * versão que funciona para ele, que está publicada logo acima, na mesma lista.
 *
 * O runbook da v0.12.0 escreveu este conserto e o deixou por fazer. Ele voltou
 * na subida para a API 5.
 *
 * **Sem a lista, não filtra.** Se a resposta do Rust não chegou, `apisDeModAceitas`
 * está vazia, e aí o comportamento é o de antes: a última da lista. Filtrar por
 * uma lista vazia esconderia o catálogo inteiro, que é pior do que oferecer uma
 * versão que o produto recusa com uma frase.
 */
function ultimaQueEsteBuildEntende(versoes) {
  const lista = versoes ?? [];
  if (apisDeModAceitas.length === 0) return lista[lista.length - 1];
  for (let i = lista.length - 1; i >= 0; i--) {
    if (apisDeModAceitas.includes(lista[i].api)) return lista[i];
  }
  return undefined;
}

/** Desenha uma linha do catálogo. */
function linhaDoCatalogo(mod, instalados) {
  const linha = elemento("li");
  const caixa = elemento("div", "server-dispositivo mods-linha-gestao");
  const texto = elemento("span", "server-dispositivo-nome");

  texto.append(elemento("span", "mods-id", mod.titulo || mod.id));
  texto.append(elemento("span", "mods-versao", mod.id));
  if (mod.resumo) texto.append(elemento("span", "mods-versao", mod.resumo));

  // **A última versão, e o selo dela — não o do MOD.** O `nivel` do topo é a
  // avaliação mais recente e serve para listar; quem carimba um número de
  // versão é o `nivel` de dentro daquela versão, porque o veredito ao lado de
  // um hash tem de ser o veredito daqueles bytes.
  const ultima = ultimaQueEsteBuildEntende(mod.versoes);
  if (!ultima) {
    texto.append(elemento("span", "mods-recusado", "sem versão publicada"));
    caixa.append(texto);
    linha.append(caixa);
    return linha;
  }
  texto.append(
    elemento("span", "mods-versao", `versão ${ultima.versao} · ${NIVEIS[ultima.nivel] ?? ultima.nivel}`),
  );
  const alcances = elemento("div", "mods-alcances");
  for (const alcance of ultima.alcanca ?? []) {
    alcances.append(elemento("span", "mods-etiqueta", alcance));
  }
  texto.append(alcances);
  caixa.append(texto);

  // **A09 da auditoria: instalado não é a mesma coisa que igual.**
  //
  // A decisão saía só de `i.id === mod.id`, e versão e hash não entravam nela.
  // Quem tinha uma versão diferente da que o servidor exige via JÁ INSTALADO e
  // nenhum caminho: instalar de novo era recusado, e o conserto virava apagar
  // pasta à mão.
  const instalado = instalados.find(i => i.id === mod.id && i.hash === ultima.hash)
    ?? instalados.find(i => i.id === mod.id);
  const mesmoConteudo = instalado?.hash === ultima.hash;
  if (instalado && mesmoConteudo) {
    caixa.append(elemento("span", "server-dispositivo-marca", "JÁ INSTALADO"));
  } else {
    if (instalado) {
      texto.append(
        elemento(
          "span",
          "mods-recusado",
          `você tem a versão ${instalado.version || "?"}; esta é a ${ultima.versao}`,
        ),
      );
    }
    const botao = elemento("button", "botao-fantasma");
    botao.type = "button";
    // Nomear a versão de destino evita chamar downgrade de atualização,
    // inclusive quando versões locais não obedecem à mesma ordenação.
    botao.textContent = instalado
      ? (instalado.version === ultima.versao ? "REINSTALAR" : `INSTALAR ${ultima.versao}`)
      : "INSTALAR";
    botao.addEventListener("click", () => {
      // **Desabilitado enquanto baixa** — A11. Sem isto, dois cliques começam
      // duas instalações sobre o mesmo destino.
      botao.disabled = true;
      instalarDoCatalogo(mod.id, ultima.versao)
        .catch((falha) => {
          console.warn("instalar do catálogo:", falha);
          const estado = $("catalogo-estado");
          estado.classList.add("mods-recusado");
          estado.textContent = fraseDeErro(falha);
        })
        .finally(() => {
          botao.disabled = false;
        });
    });
    caixa.append(botao);
  }

  linha.append(caixa);
  return linha;
}

/** Busca o catálogo e desenha o que veio. */
async function buscarOCatalogo() {
  const botao = $("catalogo-buscar");
  const estado = $("catalogo-estado");
  botao.disabled = true;
  estado.classList.remove("mods-recusado");
  estado.textContent = "buscando…";
  try {
    catalogoEmMaos = await invoke("catalogo_de_mods");
  } catch (falha) {
    estado.classList.add("mods-recusado");
    estado.textContent = fraseDeErro(falha);
    botao.disabled = false;
    return;
  }
  botao.disabled = false;
  await desenharCatalogoEmMaos();
  const mods = catalogoEmMaos.mods ?? [];
  estado.textContent =
    mods.length === 0
      ? "o catálogo está no ar e ainda não lista nenhum MOD."
      : `${mods.length} no catálogo.`;
}

/**
 * Desenha a lista que já está em mãos, sem ir à rede.
 *
 * Separado da busca por causa do A11: depois de instalar, o que mudou é o que
 * está **nesta máquina**, e não o catálogo. Buscar de novo só para redesenhar
 * dava à rede a chance de apagar, com um erro de consulta, a frase que dizia
 * que a instalação tinha dado certo.
 */
async function desenharCatalogoEmMaos() {
  let instalados = [];
  try {
    instalados = await invoke("mods_instalados");
  } catch {
    instalados = [];
  }
  repovoar(
    $("lista-catalogo"),
    (catalogoEmMaos?.mods ?? []).map((mod) => linhaDoCatalogo(mod, instalados)),
  );
}

/** Baixa e instala uma versão do catálogo. */
async function instalarDoCatalogo(id, versao) {
  const estado = $("catalogo-estado");
  estado.classList.remove("mods-recusado");
  estado.textContent = `baixando ${id} ${versao}…`;
  let resultado;
  try {
    // **QA-03: a frase era fixa e mentia na metade dos casos.** Ela dizia
    // «instalado, e desligado» também quando a atualização acabara de reaplicar
    // a exigência num MOD que estava ligado. O comando devolve o que de fato
    // aconteceu, e a frase passa a sair daí.
    const reaplicado = await invoke("instalar_mod_do_catalogo", { id, versao });
    resultado = reaplicado
      ? `${id} ${versao} instalado e aplicado neste servidor. Quem estava dentro ` +
        `precisa entrar de novo e aceitar — o conjunto mudou.`
      : `${id} ${versao} instalado, e desligado. Ligue quando quiser que ele valha.`;
  } catch (falha) {
    estado.classList.add("mods-recusado");
    estado.textContent = fraseDeErro(falha);
    await desenharMods();
    return;
  }
  estado.textContent = resultado;
  catalogoJaRedesenhou = false;
  await desenharMods();

  // **A11: o resultado da instalação não é apagado pelo estado da consulta.**
  //
  // Aqui havia um `buscarOCatalogo()`, que escreve «buscando…» e depois a
  // contagem — por cima da frase que dizia que a instalação deu certo. Se a
  // rede caísse logo depois de instalar, quem lia via um erro de catálogo e
  // concluía que a instalação falhara.
  //
  // E não é preciso ir à rede: a lista já está em `catalogoEmMaos`, e o que
  // mudou foi o que está instalado nesta máquina. Redesenhar basta.
  if (catalogoEmMaos) {
    await desenharCatalogoEmMaos();
    estado.textContent = resultado;
  }
}

// **Uma falha aqui tem de chegar à tela.** Ela já chegou só ao console uma
// vez: a contagem é escrita antes da lista, então quem usava lia «1 no
// catálogo» ao lado de lista nenhuma e não tinha o que fazer com isso. Um
// `console.warn` num aplicativo empacotado é uma mensagem para ninguém.
$("catalogo-buscar").addEventListener("click", () => {
  buscarOCatalogo().catch((falha) => {
    console.warn("catálogo:", falha);
    const estado = $("catalogo-estado");
    estado.classList.add("mods-recusado");
    estado.textContent = fraseDeErro(falha);
    $("catalogo-buscar").disabled = false;
  });
});

// O painel redesenha quando uma fase muda, e não só quando alguém o abre: um
// MOD que falha quatro segundos depois de a tela estar aberta é justamente o
// caso em que ninguém vai reabrir para descobrir.
globalThis.addEventListener("seele-mods-estado", () => {
  if ($("tela-server").hidden) return;
  desenharMods().catch((falha) => console.warn("mods:", falha));
});


// ------------------------------------------------- as abas da gestão de MODs

/**
 * Quatro atividades, quatro abas — U12 e U32.
 *
 * «Gestão de MODs junta ativação, catálogo, cache, consentimentos e contadores
 * técnicos numa página longa», e a consequência: «Pacotes repetidos em
 * instalados, catálogo e disco». O mesmo pacote aparecia três vezes na mesma
 * rolagem, e achar um MOD instalado para ligá-lo passava pelas outras duas.
 *
 * **Ativação manual**, como o renderer dos MODs faz e pela mesma razão: o
 * catálogo vai à rede, e percorrer as abas com a seta não pode disparar uma
 * busca por tecla. Seguindo o padrão de abas da WAI-ARIA APG.
 */
function ligarAsAbasDeMods() {
  const tiras = document.querySelector(".mods-tiras");
  if (!tiras || tiras.dataset.ligada === "sim") return;
  tiras.dataset.ligada = "sim";

  const botoes = () => Array.from(tiras.querySelectorAll("[role=\"tab\"]"));

  const abrir = (chave) => {
    for (const botao of botoes()) {
      const ativa = botao.dataset.aba === chave;
      botao.setAttribute("aria-selected", ativa ? "true" : "false");
      // Uma parada de tabulação para o grupo, como o APG pede: Tab entra e sai
      // das abas, e as setas percorrem por dentro.
      botao.tabIndex = ativa ? 0 : -1;
      const painel = $(`mods-painel-${botao.dataset.aba}`);
      if (painel) painel.hidden = !ativa;
    }
  };

  tiras.addEventListener("click", (evento) => {
    const alvo = evento.target.closest?.("[data-aba]");
    if (alvo) abrir(alvo.dataset.aba);
  });

  tiras.addEventListener("keydown", (evento) => {
    const lista = botoes();
    const atual = lista.indexOf(document.activeElement);
    if (atual < 0) return;
    const passo = evento.key === "ArrowRight" ? 1
      : evento.key === "ArrowLeft" ? -1
        : evento.key === "Home" ? -atual
          : evento.key === "End" ? lista.length - 1 - atual
            : 0;
    if (!passo) return;
    evento.preventDefault();
    lista[(atual + passo + lista.length) % lista.length]?.focus();
  });
}

ligarAsAbasDeMods();

// A lista de APIs que este build aceita, para `ultimaQueEsteBuildEntende`.
// Pedida uma vez, na carga: ela não muda enquanto o aplicativo roda.
invoke("apis_de_mod_aceitas")
  .then((apis) => {
    apisDeModAceitas = apis;
  })
  .catch((falha) => console.warn("apis_de_mod_aceitas:", falha));
