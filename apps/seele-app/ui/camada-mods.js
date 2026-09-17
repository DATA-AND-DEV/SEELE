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
  aceitePendente = { alvo, conjunto };
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

/** Grava o sim e tenta entrar de novo. */
async function aceitarOsMods() {
  if (!aceitePendente) return;
  const { alvo, conjunto } = aceitePendente;
  const botao = $("mods-aceitar");
  botao.disabled = true;
  try {
    await invoke("aceitar_mods", { alvo, conjunto });
  } catch (falha) {
    // O sim não foi gravado. **Não tenta entrar**: entrar agora falharia de
    // novo pelo mesmo motivo, e a pessoa leria a mesma pergunta duas vezes sem
    // saber que o problema foi gravar.
    botao.disabled = false;
    const erro = $("mods-erro");
    erro.hidden = false;
    erro.textContent = fraseDeErro(falha);
    return;
  }
  fecharAceiteDeMods();
  // A mesma porta de sempre. O aceite já está em disco, então esta tentativa
  // passa pelo ponto que recusou a anterior.
  await conectar();
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
  if (mod.server) {
    texto.append(
      elemento("span", "mods-servidor", "roda na máquina de quem hospeda"),
    );
  }
  caixa.append(texto);

  const botao = elemento("button", "botao-fantasma");
  botao.type = "button";
  botao.textContent = mod.enabled ? "DESLIGAR" : "LIGAR";
  // Sem hospedar não há servidor em que ligar. Desabilitado **e** explicado
  // logo abaixo da lista: um botão morto sem motivo é uma pergunta sem resposta.
  botao.disabled = !hospedando;
  botao.addEventListener("click", () => {
    trocarOMod(mod.id, !mod.enabled).catch((falha) =>
      console.warn("trocar mod:", falha),
    );
  });
  caixa.append(botao);

  linha.append(caixa);
  return linha;
}

/** Liga ou desliga, e redesenha. */
async function trocarOMod(id, ligar) {
  const erro = $("mods-gestao-erro");
  erro.hidden = true;
  try {
    // Os dois nomes escritos por extenso, e não um `invoke(condição ? a : b)`.
    // Um nome de comando computado é invisível para
    // `no_command_is_registered_and_never_called`, que existe justamente para
    // achar o comando que ninguém chama — e um guarda cego é pior que guarda
    // nenhum, porque ele continua verde.
    if (ligar) {
      await invoke("habilitar_mod", { id });
    } else {
      await invoke("desabilitar_mod", { id });
    }
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
}

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

  const jaTem = instalados.some((i) => i.id === mod.id);
  if (jaTem) {
    caixa.append(elemento("span", "server-dispositivo-marca", "JÁ INSTALADO"));
  } else {
    const botao = elemento("button", "botao-fantasma");
    botao.type = "button";
    botao.textContent = "INSTALAR";
    botao.addEventListener("click", () => {
      instalarDoCatalogo(mod.id, ultima.versao).catch((falha) => {
        console.warn("instalar do catálogo:", falha);
        const estado = $("catalogo-estado");
        estado.classList.add("mods-recusado");
        estado.textContent = fraseDeErro(falha);
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

  let instalados = [];
  try {
    instalados = await invoke("mods_instalados");
  } catch {
    instalados = [];
  }

  const mods = catalogoEmMaos.mods ?? [];
  estado.textContent =
    mods.length === 0
      ? "o catálogo está no ar e ainda não lista nenhum MOD."
      : `${mods.length} no catálogo.`;
  repovoar(
    $("lista-catalogo"),
    mods.map((mod) => linhaDoCatalogo(mod, instalados)),
  );
}

/** Baixa e instala uma versão do catálogo. */
async function instalarDoCatalogo(id, versao) {
  const estado = $("catalogo-estado");
  estado.classList.remove("mods-recusado");
  estado.textContent = `baixando ${id} ${versao}…`;
  try {
    await invoke("instalar_mod_do_catalogo", { id, versao });
    estado.textContent = `${id} ${versao} instalado, e desligado. Ligue quando quiser que ele valha.`;
  } catch (falha) {
    estado.classList.add("mods-recusado");
    estado.textContent = fraseDeErro(falha);
  }
  await desenharMods();
  // A lista do catálogo mostra JÁ INSTALADO, e ela acabou de mudar.
  if (catalogoEmMaos) await buscarOCatalogo();
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
