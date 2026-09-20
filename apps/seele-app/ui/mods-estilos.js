// O estilo que um MOD declara, e o que o produto deixa virar pintura.
//
// ADR 0052. A auditoria de 20/09/2026 disse a coisa toda numa frase: «Não
// limitar o visual de um perfil a seis cores e dois enums.» Este arquivo é a
// resposta — e a parte que **não** é a resposta está igualmente escrita aqui.
//
// # A fronteira, dita pelo que ela é
//
// A fronteira nunca foi «cantos arredondados são feios». Ela é **alcance**: um
// estilo de um MOD não pode alcançar a janela inteira, não pode buscar bytes na
// rede, não pode cobrir uma confirmação de confiança do produto e não pode
// sobreviver à saída do servidor.
//
// Nada disso exige proibir gradiente, sombra ou animação. Exige que o valor de
// cada propriedade seja **construído aqui** a partir de partes que este arquivo
// conferiu, em vez de copiado de um texto que veio de fora. É a diferença entre
// aceitar `sombra: { x: 0, y: 4, desfoque: 12, cor: '#00000055' }` e aceitar
// `box-shadow: <texto>` — a segunda aceita `url(...)`, aceita uma vírgula com
// mais uma sombra atrás, e aceita o que o motor de CSS aceitar no dia em que
// ele mudar.
//
// Por isso não há aqui nenhuma função que receba CSS e tente limpá-lo. Limpeza
// por substituição de texto é uma corrida contra o analisador do navegador, e
// quem escreve o analisador não sabe que esta corrida existe.
//
// # O que o MOD ganha
//
// Cor com transparência, gradiente declarado por paradas, borda, raio, sombra,
// tipografia inteira, layout de caixa/pilha/linha/grade, transformação,
// recorte, transição e animação por preset. Estados (`hover`, `focus-visible`,
// `active`, `disabled`, `invalid`, `selected`) e consultas de tamanho **do
// contêiner** — porque a medida da janela não serve para um painel que a pessoa
// arrastou para 300 px.

/**
 * Os tetos dos números que um estilo aceita.
 *
 * Cada um existe por uma razão que não é gosto:
 *
 * - `corpo` tem piso porque texto de 4 px não é uma escolha estética, é texto
 *   que ninguém lê — e o MOD pode estar desenhando a identidade de alguém;
 * - `sombra` tem teto de desfoque e de deslocamento porque uma sombra de
 *   4000 px pinta a tela inteira a partir de um nó de 10 px;
 * - `girar` e `escalar` têm teto pela mesma razão: `scale(400)` é um retângulo
 *   que cobre a janela;
 * - `paradas` de gradiente têm teto porque cada parada é trabalho de
 *   composição, e mil delas num nó que repinta a cada quadro é o orçamento de
 *   GPU de quem está numa chamada de voz.
 */
const LIMITES_DO_ESTILO = Object.freeze({
  corpo: { minimo: 9, maximo: 96 },
  entrelinha: { minimo: 0.8, maximo: 3 },
  espacamento: { minimo: -2, maximo: 8 },
  raio: { minimo: 0, maximo: 999 },
  bordaLargura: { minimo: 0, maximo: 16 },
  sombraDeslocamento: { minimo: -64, maximo: 64 },
  sombraDesfoque: { minimo: 0, maximo: 96 },
  sombraEspalha: { minimo: -32, maximo: 32 },
  opacidade: { minimo: 0, maximo: 1 },
  medida: { minimo: 0, maximo: 4096 },
  espaco: { minimo: 0, maximo: 256 },
  girar: { minimo: -360, maximo: 360 },
  escalar: { minimo: 0.1, maximo: 4 },
  mover: { minimo: -512, maximo: 512 },
  duracao: { minimo: 0, maximo: 4000 },
  colunas: { minimo: 1, maximo: 12 },
  paradas: 8,
  classes: 64,
  consultas: 4,
  regrasPorSuperficie: 512,
});

/** Uma cor `#rgb`, `#rrggbb`, `#rrggbbaa`. Nada mais: nomes e funções não entram. */
const COR_DE_MOD = /^#(?:[0-9a-f]{3}|[0-9a-f]{6}|[0-9a-f]{8})$/i;

/**
 * As palavras que cada propriedade de enumeração aceita, e o CSS de cada uma.
 *
 * **Tabela e não passagem.** O valor que sai daqui é uma constante deste
 * arquivo; o que veio do MOD só escolheu qual. É o que faz `alinhar: 'center'`
 * não ser um caminho para escrever qualquer coisa em `align-items`.
 */
const PALAVRAS_DO_ESTILO = Object.freeze({
  direcao: { linha: "row", coluna: "column", linhaInversa: "row-reverse", colunaInversa: "column-reverse" },
  alinhar: { inicio: "flex-start", centro: "center", fim: "flex-end", esticar: "stretch", base: "baseline" },
  distribuir: {
    inicio: "flex-start", centro: "center", fim: "flex-end",
    entre: "space-between", ao_redor: "space-around", igual: "space-evenly",
  },
  quebra: { sim: "wrap", nao: "nowrap" },
  familia: { mono: "var(--seele-mono)", sans: "var(--seele-sans)" },
  peso: { leve: "300", normal: "400", medio: "500", forte: "700" },
  alinhamento: { esquerda: "left", centro: "center", direita: "right", justificado: "justify" },
  transformar: { nenhuma: "none", maiuscula: "uppercase", minuscula: "lowercase", capitular: "capitalize" },
  recortar: { nenhum: "visible", cortar: "hidden", rolar: "auto" },
  bordaEstilo: { solida: "solid", tracejada: "dashed", pontilhada: "dotted", nenhuma: "none" },
  suavizacao: {
    linear: "linear", entrada: "ease-in", saida: "ease-out",
    entradaSaida: "ease-in-out", firme: "cubic-bezier(0.2, 0, 0, 1)",
  },
  posicao: { fluxo: "static", relativa: "relative", presa: "sticky" },
});

/**
 * As animações que o produto sabe fazer, por nome.
 *
 * **Presets e não keyframes livres**, e a razão é a mesma da sombra: um
 * `@keyframes` declarado por um MOD é um texto que vira regra CSS, e a lista do
 * que uma regra pode conter não é nossa. Estas quatro cobrem o que a auditoria
 * pediu de volta — o PERFIS antigo tinha aurora, brilho e pulso — e são
 * compostas de propriedades que este arquivo já valida.
 *
 * Todas param sob `prefers-reduced-motion`: ver `mods-estilos.css`.
 */
const ANIMACOES_DO_MOD = Object.freeze({
  nenhuma: "",
  pulso: "seele-mod-pulso",
  aurora: "seele-mod-aurora",
  brilho: "seele-mod-brilho",
  flutuar: "seele-mod-flutuar",
});

/** Um número dentro de um intervalo, ou `null` quando não é número. */
function numeroNoIntervalo(valor, intervalo) {
  const n = Number(valor);
  if (!Number.isFinite(n)) return null;
  return Math.min(Math.max(n, intervalo.minimo), intervalo.maximo);
}

/** Uma cor que passou pelo formato, ou `null`. */
function corDeMod(valor) {
  return typeof valor === "string" && COR_DE_MOD.test(valor) ? valor : null;
}

/** Uma palavra de uma tabela, traduzida no valor do produto, ou `null`. */
function palavraDoEstilo(grupo, valor) {
  const tabela = PALAVRAS_DO_ESTILO[grupo];
  if (!tabela) return null;
  return typeof valor === "string" && Object.hasOwn(tabela, valor) ? tabela[valor] : null;
}

/**
 * Uma medida: número em pixels, `'total'` (100%), ou uma fração `{ de: n }`.
 *
 * Porcentagem entra por `{ de: 50 }` e não por `'50%'` pelo motivo de sempre:
 * um número é conferível e uma string é um analisador.
 */
function medidaDeMod(valor) {
  if (valor === "total") return "100%";
  if (valor === "conteudo") return "max-content";
  if (valor && typeof valor === "object" && !Array.isArray(valor)) {
    const fracao = numeroNoIntervalo(valor.de, { minimo: 0, maximo: 100 });
    return fracao === null ? null : `${fracao}%`;
  }
  const px = numeroNoIntervalo(valor, LIMITES_DO_ESTILO.medida);
  return px === null ? null : `${px}px`;
}

/** Um gradiente declarado por paradas — nunca por texto de `linear-gradient`. */
function gradienteDeMod(declarado) {
  if (!declarado || typeof declarado !== "object") return null;
  const paradas = (Array.isArray(declarado.paradas) ? declarado.paradas : [])
    .slice(0, LIMITES_DO_ESTILO.paradas)
    .map((parada) => {
      const cor = corDeMod(parada?.cor);
      if (!cor) return null;
      const em = numeroNoIntervalo(parada?.em, { minimo: 0, maximo: 100 });
      return em === null ? cor : `${cor} ${em}%`;
    })
    .filter(Boolean);
  if (paradas.length < 2) return null;
  if (declarado.tipo === "radial") return `radial-gradient(circle, ${paradas.join(", ")})`;
  const angulo = numeroNoIntervalo(declarado.angulo, { minimo: 0, maximo: 360 }) ?? 180;
  return `linear-gradient(${angulo}deg, ${paradas.join(", ")})`;
}

/** Uma sombra montada de partes conferidas. */
function sombraDeMod(declarada) {
  if (declarada === false || declarada === "nenhuma") return "none";
  if (!declarada || typeof declarada !== "object") return null;
  const cor = corDeMod(declarada.cor) ?? "#00000066";
  const x = numeroNoIntervalo(declarada.x, LIMITES_DO_ESTILO.sombraDeslocamento) ?? 0;
  const y = numeroNoIntervalo(declarada.y, LIMITES_DO_ESTILO.sombraDeslocamento) ?? 0;
  const desfoque = numeroNoIntervalo(declarada.desfoque, LIMITES_DO_ESTILO.sombraDesfoque) ?? 0;
  const espalha = numeroNoIntervalo(declarada.espalha, LIMITES_DO_ESTILO.sombraEspalha) ?? 0;
  const dentro = declarada.dentro === true ? "inset " : "";
  return `${dentro}${x}px ${y}px ${desfoque}px ${espalha}px ${cor}`;
}

/**
 * Um estilo declarado, virado num par de listas de `[propriedade, valor]`.
 *
 * **Devolve pares e não texto.** Quem aplica usa `style.setProperty`, que só
 * aceita o par; montar uma string de CSS aqui criaria, no meio do caminho, um
 * texto que alguém pode vir a concatenar. A lista é a forma que não tem esse
 * meio do caminho.
 *
 * O que não for reconhecido **some, e é contado**: um MOD que escreveu
 * `corDeFundo` em vez de `fundo` precisa descobrir isso, e a contagem é o que
 * a região devolve como recusa.
 *
 * @param {object} estilo O que o MOD declarou.
 * @param {{ recusados: number }} [conta] Onde somar o que não foi reconhecido.
 * @returns {Array<[string, string]>} Os pares prontos para `setProperty`.
 */
function estiloDeMod(estilo, conta = null) {
  if (!estilo || typeof estilo !== "object" || Array.isArray(estilo)) return [];
  const pares = [];
  const recusar = () => { if (conta) conta.recusados += 1; };
  const por = (propriedade, valor) => {
    if (valor === null || valor === undefined) { recusar(); return; }
    pares.push([propriedade, valor]);
  };

  for (const [chave, valor] of Object.entries(estilo)) {
    if (valor === undefined) continue;
    switch (chave) {
      // ---- cor e preenchimento ----
      case "cor": por("color", corDeMod(valor)); break;
      case "fundo": por("background-color", corDeMod(valor)); break;
      case "gradiente": por("background-image", gradienteDeMod(valor)); break;
      case "opacidade": {
        const n = numeroNoIntervalo(valor, LIMITES_DO_ESTILO.opacidade);
        por("opacity", n === null ? null : String(n));
        break;
      }

      // ---- borda, raio e sombra ----
      case "raio": {
        const n = numeroNoIntervalo(valor, LIMITES_DO_ESTILO.raio);
        por("border-radius", n === null ? null : `${n}px`);
        break;
      }
      case "borda": {
        if (valor === false || valor === "nenhuma") { por("border", "none"); break; }
        if (!valor || typeof valor !== "object") { recusar(); break; }
        const largura = numeroNoIntervalo(valor.largura, LIMITES_DO_ESTILO.bordaLargura) ?? 1;
        const forma = palavraDoEstilo("bordaEstilo", valor.estilo) ?? "solid";
        const cor = corDeMod(valor.cor) ?? "currentColor";
        // Montada de três partes conferidas, e não de uma string do MOD.
        por("border", `${largura}px ${forma} ${cor}`);
        break;
      }
      case "sombra": por("box-shadow", sombraDeMod(valor)); break;

      // ---- tipografia ----
      case "familia": por("font-family", palavraDoEstilo("familia", valor)); break;
      case "peso": por("font-weight", palavraDoEstilo("peso", valor)); break;
      case "corpo": {
        const n = numeroNoIntervalo(valor, LIMITES_DO_ESTILO.corpo);
        por("font-size", n === null ? null : `${n}px`);
        break;
      }
      case "entrelinha": {
        const n = numeroNoIntervalo(valor, LIMITES_DO_ESTILO.entrelinha);
        por("line-height", n === null ? null : String(n));
        break;
      }
      case "espacamento": {
        const n = numeroNoIntervalo(valor, LIMITES_DO_ESTILO.espacamento);
        por("letter-spacing", n === null ? null : `${n}px`);
        break;
      }
      case "alinhamento": por("text-align", palavraDoEstilo("alinhamento", valor)); break;
      case "transformar": por("text-transform", palavraDoEstilo("transformar", valor)); break;

      // ---- layout ----

      // **Declarar uma direção é pedir uma linha (ou uma coluna).**
      //
      // `direcao` escrevia só `flex-direction`, que não faz nada num elemento
      // que não é contêiner flex. `caixa` não é: a folha lhe dá `min-width: 0`
      // e nada mais. Então `caixa([a, b], { direcao: 'linha' })` — que é como
      // os três MODs oficiais pedem campos e prévia lado a lado — empilhava,
      // e a intenção declarada não produzia a composição.
      //
      // A validação nativa de 20/09/2026 e o reteste de `c4fe3ea` mediram o
      // efeito nos dois: «amostras e controles que pedem `direcao: 'linha'`
      // continuam empilhados», «a composição desperdiça altura».
      //
      // `display: flex` sai junto. É o que a palavra quer dizer, e escrevê-lo
      // aqui vale mais do que documentar que `direcao` só serve em `pilha`:
      // uma API cuja forma correta depende de saber qual primitiva já é flex
      // é uma API que ensina pelo erro.
      case "direcao":
        por("display", "flex");
        por("flex-direction", palavraDoEstilo("direcao", valor));
        break;
      case "alinhar": por("align-items", palavraDoEstilo("alinhar", valor)); break;
      case "distribuir": por("justify-content", palavraDoEstilo("distribuir", valor)); break;
      case "quebra": por("flex-wrap", palavraDoEstilo("quebra", valor)); break;
      case "crescer": {
        const n = numeroNoIntervalo(valor, { minimo: 0, maximo: 16 });
        por("flex-grow", n === null ? null : String(n));
        break;
      }
      case "encolher": {
        const n = numeroNoIntervalo(valor, { minimo: 0, maximo: 16 });
        por("flex-shrink", n === null ? null : String(n));
        break;
      }

      // **O tamanho de partida numa linha.** Sem ele, `crescer: 1` em duas
      // caixas de uma linha que quebra não as divide: cada uma parte do
      // tamanho do próprio conteúdo, o par não cabe, e a linha vira duas —
      // que foi o que o editor do PERFIS e as amostras do ESTILO fizeram.
      //
      // `base: 0` com `larguraMinima` é o par que divide e ainda quebra:
      // enquanto as duas mínimas couberem, elas dividem o que sobra; quando
      // não couberem, a linha quebra sozinha, sem depender de consulta nenhuma.
      case "base": por("flex-basis", medidaDeMod(valor)); break;
      case "intervalo": {
        const n = numeroNoIntervalo(valor, LIMITES_DO_ESTILO.espaco);
        por("gap", n === null ? null : `${n}px`);
        break;
      }
      case "preenchimento": {
        const n = numeroNoIntervalo(valor, LIMITES_DO_ESTILO.espaco);
        por("padding", n === null ? null : `${n}px`);
        break;
      }
      case "margem": {
        const n = numeroNoIntervalo(valor, LIMITES_DO_ESTILO.espaco);
        por("margin", n === null ? null : `${n}px`);
        break;
      }
      case "largura": por("width", medidaDeMod(valor)); break;
      case "altura": por("height", medidaDeMod(valor)); break;
      case "larguraMinima": por("min-width", medidaDeMod(valor)); break;
      case "larguraMaxima": por("max-width", medidaDeMod(valor)); break;
      case "alturaMinima": por("min-height", medidaDeMod(valor)); break;
      case "alturaMaxima": por("max-height", medidaDeMod(valor)); break;
      case "colunas": {
        const n = numeroNoIntervalo(valor, LIMITES_DO_ESTILO.colunas);
        // `1fr` por coluna: o MOD escolhe **quantas**, e o produto escolhe como
        // elas se dividem. Uma `grid-template-columns` livre aceita `minmax`
        // com o que vier dentro.
        por("grid-template-columns", n === null ? null : `repeat(${Math.round(n)}, minmax(0, 1fr))`);
        break;
      }
      case "posicao": por("position", palavraDoEstilo("posicao", valor)); break;
      case "recortar": por("overflow", palavraDoEstilo("recortar", valor)); break;
      case "proporcao": {
        const n = numeroNoIntervalo(valor, { minimo: 0.1, maximo: 10 });
        por("aspect-ratio", n === null ? null : String(n));
        break;
      }

      // ---- transformação ----
      case "girar": {
        const n = numeroNoIntervalo(valor, LIMITES_DO_ESTILO.girar);
        por("rotate", n === null ? null : `${n}deg`);
        break;
      }
      case "escalar": {
        const n = numeroNoIntervalo(valor, LIMITES_DO_ESTILO.escalar);
        por("scale", n === null ? null : String(n));
        break;
      }
      case "mover": {
        if (!valor || typeof valor !== "object") { recusar(); break; }
        const x = numeroNoIntervalo(valor.x, LIMITES_DO_ESTILO.mover) ?? 0;
        const y = numeroNoIntervalo(valor.y, LIMITES_DO_ESTILO.mover) ?? 0;
        por("translate", `${x}px ${y}px`);
        break;
      }

      // ---- movimento ----
      case "transicao": {
        if (!valor || typeof valor !== "object") { recusar(); break; }
        const duracao = numeroNoIntervalo(valor.duracao, LIMITES_DO_ESTILO.duracao) ?? 160;
        const suave = palavraDoEstilo("suavizacao", valor.suavizacao) ?? "cubic-bezier(0.2, 0, 0, 1)";
        // `all` de propósito: o que muda é decidido pelos outros pares desta
        // mesma lista, que já passaram por aqui.
        por("transition", `all ${duracao}ms ${suave}`);
        break;
      }
      case "animacao": {
        const nome = typeof valor === "string" ? valor : valor?.nome;
        const quadro = Object.hasOwn(ANIMACOES_DO_MOD, nome) ? ANIMACOES_DO_MOD[nome] : null;
        if (quadro === null) { recusar(); break; }
        if (!quadro) { por("animation", "none"); break; }
        const duracao = numeroNoIntervalo(valor?.duracao, { minimo: 200, maximo: 8000 }) ?? 2400;
        por("animation", `${quadro} ${duracao}ms ease-in-out infinite`);
        break;
      }

      default:
        // Uma propriedade que este arquivo não conhece **não vira CSS**. Não
        // por desconfiança do nome, mas porque aceitar o desconhecido é
        // exatamente o caminho que devolve `box-shadow: <texto>` por outra
        // porta.
        recusar();
        break;
    }
  }
  return pares;
}

/**
 * Aplica um estilo declarado num elemento, e devolve o que ele escreveu.
 *
 * Devolve a lista de propriedades para a reconciliação poder **apagar** as que
 * saíram: sem isso, tirar `fundo` de uma declaração deixaria a cor anterior na
 * tela para sempre, e o MOD não teria como desfazer o que escreveu.
 */
function aplicarEstiloDeMod(elem, estilo, conta = null) {
  const pares = estiloDeMod(estilo, conta);
  const novas = new Set(pares.map(([p]) => p));
  const antigas = elem.dataset.estiloDeMod ? elem.dataset.estiloDeMod.split(" ") : [];
  for (const propriedade of antigas) {
    if (propriedade && !novas.has(propriedade)) elem.style.removeProperty(propriedade);
  }
  for (const [propriedade, valor] of pares) elem.style.setProperty(propriedade, valor);
  if (novas.size) elem.dataset.estiloDeMod = [...novas].join(" ");
  else delete elem.dataset.estiloDeMod;
  return pares.length;
}

/** Os estados que uma classe de MOD pode declarar, e o seletor de cada um. */
const ESTADOS_DA_CLASSE = Object.freeze({
  base: "",
  sobre: ":hover",
  foco: ":focus-visible",
  ativo: ":active",
  desligado: ":disabled,PREFIXO[aria-disabled=\"true\"]",
  invalido: "[data-invalido=\"sim\"]",
  escolhido: "[data-escolhido=\"sim\"]",
});

/**
 * Compila as classes de uma superfície numa folha de estilo **dela**.
 *
 * # Por que classes, e não só estilo por nó
 *
 * Estado. `hover`, `focus-visible` e `active` não existem como propriedade de
 * um nó: eles são seletores, e seletor é regra. Um MOD que quisesse um botão
 * que muda de cor ao passar o mouse teria de receber evento de `pointerover`,
 * responder com uma árvore nova e pagar uma travessia da ponte por movimento do
 * mouse — que é precisamente o que o §5 do plano proíbe.
 *
 * # Por que isto não é CSS do MOD na janela
 *
 * O texto da regra é montado aqui, de partes que `estiloDeMod` conferiu, com um
 * seletor que **este arquivo** escreve. O MOD escolhe um nome de classe, e o
 * nome é higienizado e prefixado com a identidade da superfície: duas
 * superfícies com a classe `cartao` não se alcançam, e nenhuma das duas alcança
 * `.roster-linha`.
 *
 * @param {string} escopo O atributo que prende as regras a esta superfície.
 * @param {object} classes `{ nome: { base, sobre, foco, ..., consultas } }`.
 * @param {{ recusados: number }} [conta]
 * @returns {string} O texto da folha, pronto para um `<style>` do produto.
 */
function folhaDeClassesDeMod(escopo, classes, conta = null) {
  if (!classes || typeof classes !== "object") return "";
  const nomes = Object.entries(classes).slice(0, LIMITES_DO_ESTILO.classes);
  const regras = [];
  const raiz = `[data-escopo-de-mod="${escopo}"]`;

  const corpoDaRegra = (estilo) => {
    const pares = estiloDeMod(estilo, conta);
    if (!pares.length) return "";
    // Cada valor já é uma constante deste arquivo ou um número formatado por
    // ele. Ainda assim: nada que carregue `}` ou `;` atravessa, porque nada
    // aqui vem de texto do MOD.
    return pares.map(([p, v]) => `${p}:${v}`).join(";");
  };

  for (const [nome, declarado] of nomes) {
    if (regras.length >= LIMITES_DO_ESTILO.regrasPorSuperficie) break;
    const limpo = String(nome).replace(/[^A-Za-z0-9_-]/g, "").slice(0, 48);
    if (!limpo) { if (conta) conta.recusados += 1; continue; }
    if (!declarado || typeof declarado !== "object") { if (conta) conta.recusados += 1; continue; }
    const alvo = `${raiz} .mod-${limpo}`;

    for (const [estado, sufixo] of Object.entries(ESTADOS_DA_CLASSE)) {
      const estilo = declarado[estado];
      if (!estilo) continue;
      const corpo = corpoDaRegra(estilo);
      if (!corpo) continue;
      const seletor = sufixo
        ? sufixo.split(",").map((s) => alvo + s.replace("PREFIXO", "")).join(",")
        : alvo;
      regras.push(`${seletor}{${corpo}}`);
    }

    // **Consultas do contêiner, e não da janela.** Um painel que a pessoa
    // arrastou para 300 px continua numa janela de 1400; perguntar à janela
    // daria a resposta errada exatamente onde a resposta importa.
    const consultas = Array.isArray(declarado.consultas)
      ? declarado.consultas.slice(0, LIMITES_DO_ESTILO.consultas)
      : [];
    for (const consulta of consultas) {
      const ate = numeroNoIntervalo(consulta?.ateLargura, { minimo: 120, maximo: 2400 });
      if (ate === null) { if (conta) conta.recusados += 1; continue; }
      const corpo = corpoDaRegra(consulta.estilo);
      if (!corpo) continue;
      regras.push(`@container superficie-de-mod (max-width:${Math.round(ate)}px){${alvo}{${corpo}}}`);
    }
  }
  return regras.join("\n");
}

/** O nome de classe que um nó pede, higienizado e prefixado. */
function classesDeMod(declarado) {
  const lista = Array.isArray(declarado) ? declarado : [declarado];
  return lista
    .filter((n) => typeof n === "string")
    .map((n) => n.replace(/[^A-Za-z0-9_-]/g, "").slice(0, 48))
    .filter(Boolean)
    .slice(0, 8)
    .map((n) => `mod-${n}`);
}
