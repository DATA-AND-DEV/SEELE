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
    elemento("span", "mods-versao", `versão ${mod.version}`),
  );
  // **O que aconteceu com ele, por fase** — A06 da auditoria.
  //
  // A gestão dizia instalado e ligado, e nada mais. Tudo o que podia dar errado
  // — pacote ausente, versão diferente da exigida, catálogo sem resposta,
  // script que não carregou — morria no console, e num aplicativo empacotado o
  // console não é lugar nenhum. Quem usava via o MOD na lista e nenhum botão
  // dele na tela, sem próximo passo.
  const estado = estadoDosMods.get(mod.id);
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
  const ligadoNoRascunho = rascunho.has(mod.id);
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
    if (ligadoNoRascunho) rascunho.delete(mod.id);
    else rascunho.add(mod.id);
    desenharMods().catch((falha) => console.warn("desenhar mods:", falha));
  });
  caixa.append(botao);

  linha.append(caixa);
  return linha;
}

/**
 * Pergunta antes de mudar o que a sala exige — A04 e A08 da auditoria.
 *
 * **Ligar era uma decisão que virava duas perguntas.** A exigência era gravada
 * no servidor e nenhum consentimento era registrado nesta máquina; o servidor
 * então encerra as conexões cujo conjunto mudou, e na entrada seguinte quem
 * acabara de ligar via a tela genérica de aceite, para decidir de novo o que
 * tinha acabado de decidir. Agora é uma decisão só, dita por inteiro.
 *
 * **E desligar não é uma ação sem impacto.** Ela parece mais branda que ligar e
 * derruba as mesmas sessões, pelo mesmo motivo: o conjunto mudou. A pendência
 * 44 já dizia isso.
 *
 * O que muda de verdade é dito com o dado que a ponte já manda:
 * `exigencia_vale_na_rede` responde se a exigência **barra** alguém hoje, e não
 * só se está gravada. Sem ele a frase prometeria uma tranca que pode estar
 * dormente.
 */
function perguntarETrocarOMod(mod) {
  const ligar = !mod.enabled;
  const alcance = (mod.reach ?? []).join(", ");
  const derruba = mod.exigencia_vale_na_rede;

  const linhas = ligar
    ? [
        `«${mod.id}» passa a ser exigido neste servidor.`,
        alcance ? `Ele declara alcançar: ${alcance}.` : "Ele não declara alcance nenhum.",
        mod.server
          ? "Metade dele roda na máquina de quem hospeda — a sua."
          : "Ele roda só na janela de quem entra.",
        derruba
          ? "Quem está dentro agora cai e precisa aceitar de novo, no aparelho de cada um."
          : "A exigência fica gravada, mas hoje ela não barra ninguém na rede.",
        "Ligar aqui também registra o seu sim nesta máquina, para este conjunto exato — " +
          "você não vai ser perguntado outra vez pela decisão que acabou de tomar.",
      ]
    : [
        `«${mod.id}» deixa de ser exigido neste servidor.`,
        derruba
          ? "Quem está dentro agora cai igual, e precisa entrar de novo: desligar muda o " +
            "conjunto tanto quanto ligar."
          : "Quem está dentro agora cai igual: desligar muda o conjunto tanto quanto ligar.",
        "O MOD continua instalado nesta máquina. Isto não apaga nada.",
      ];

  abrirConfirmacao(
    ligar ? "EXIGIR NESTE SERVIDOR E USAR NESTE COMPUTADOR" : "DEIXAR DE EXIGIR",
    linhas.join("\n"),
    ligar ? "LIGAR" : "DESLIGAR",
    () => trocarOMod(mod.id, ligar),
  );
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

/** O que estaria ligado se esta tela fosse salva agora. */
const rascunho = new Set();

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
  const ligar = [...rascunho].filter((id) => !conjuntoNoServidor.has(id));
  const desligar = [...conjuntoNoServidor].filter((id) => !rascunho.has(id));
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
        await invoke("aplicar_conjunto_de_mods", {
          ligados: [...rascunho],
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

/** Liga ou desliga, e redesenha. */
async function trocarOMod(id, ligar) {
  const erro = $("mods-gestao-erro");
  erro.hidden = true;
  try {
    // **Um comando, e não dois.** Mudar a exigência e registrar o sim desta
    // máquina ao conjunto resultante são a mesma decisão, e separá-los era o
    // que fazia o servidor perguntar de novo a quem acabara de responder.
    await invoke("aplicar_mod", { id, ligar });
  } catch (falha) {
    erro.hidden = false;
    erro.textContent = fraseDeErro(falha);
  }
  await desenharMods();
}

/** Desenha as duas listas desta seção. */
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

  // **O conjunto de pé, e o rascunho por cima dele.**
  //
  // O rascunho só é semeado quando não há edição pendente. Refazê-lo a cada
  // desenho — e esta tela redesenha sozinha — apagaria o que a pessoa acabou
  // de marcar, no meio de ela marcar. Se o conjunto mudar por baixo enquanto
  // há rascunho, quem avisa é a conferência de conflito do SALVAR, que diz o
  // que houve em vez de escolher sozinha qual das duas decisões vale.
  const tinhaPendencia = haRascunhoPorSalvar();
  conjuntoNoServidor = new Set(
    instalados.filter((m) => m.enabled).map((m) => m.id),
  );
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

  const lista = $("lista-mods");
  if (instalados.length === 0) {
    repovoar(lista, [
      elemento(
        "li",
        "server-dispositivos-vazio",
        "NENHUM MOD INSTALADO NESTA MÁQUINA",
      ),
    ]);
  } else {
    repovoar(
      lista,
      instalados.map((mod) => linhaDeModInstalado(mod, hospedando)),
    );
  }

  desenharRascunho(hospedando);
  await desenharAceites();
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

/** O nível de avaliação, escrito como a pessoa lê. */
const NIVEIS = {
  oficial: "OFICIAL",
  verificado: "VERIFICADO",
  "com-notas": "PUBLICADO COM NOTAS",
};

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
  const ultima = (mod.versoes ?? [])[(mod.versoes ?? []).length - 1];
  if (!ultima) {
    texto.append(elemento("span", "mods-recusado", "sem versão publicada"));
    caixa.append(texto);
    linha.append(caixa);
    return linha;
  }
  texto.append(
    elemento("span", "mods-versao", `versão ${ultima.versao} · ${NIVEIS[ultima.nivel] ?? ultima.nivel}`),
  );
  for (const alcance of ultima.alcanca ?? []) {
    texto.append(elemento("span", "mods-etiqueta", alcance));
  }
  caixa.append(texto);

  // **A09 da auditoria: instalado não é a mesma coisa que igual.**
  //
  // A decisão saía só de `i.id === mod.id`, e versão e hash não entravam nela.
  // Quem tinha uma versão diferente da que o servidor exige via JÁ INSTALADO e
  // nenhum caminho: instalar de novo era recusado, e o conserto virava apagar
  // pasta à mão.
  const instalado = instalados.find((i) => i.id === mod.id);
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
    botao.textContent = instalado ? "ATUALIZAR" : "INSTALAR";
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
