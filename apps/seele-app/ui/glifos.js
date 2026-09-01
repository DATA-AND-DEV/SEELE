// SEELE — os seis glifos que a face de dados não tem.
//
// A IBM Plex Mono embarcada (`fontes/ibm-plex-mono-400.woff2`) traz 1049
// entradas de cmap e **um** glifo em toda a faixa U+25A0–U+25CF. Ou seja: ▸ ◂
// ▼ ▶ ● ○ e ⌘ não estão nela. Cada um caía na monoespaçada do sistema — SF Mono
// no macOS, Consolas no Windows, o que houver no Linux — e o resultado é uma
// segunda face no meio de uma linha, numa interface cujo argumento inteiro é
// que toda linha é uma grade de 8×16.
//
// `docs/marca.md` já resolveu essa mesma discussão uma camada acima: os três
// katakana da marca estão em `<path>` e não em `<text>` porque como texto a
// marca seria Hiragino no macOS e Yu Gothic no Windows. O argumento é o mesmo
// aqui, e a resposta também: desenho, não caractere.
//
// ---- tamanho ----
//
// A caixa é sempre `1em` quadrada, com `viewBox` de 16 unidades, e o tamanho
// ótico de cada figura é escolhido **dentro** dela. Não são coisas do mesmo
// peso, e o Unicode concorda: ▸ e ◂ são as formas *small*, ▼ e ▶ as grandes.
//
//   recuar/avancar   5×7 de 16   o cursor do compor e as setas da busca
//   aberto/fechado   9×6 de 16   a sala de voz aberta ou fechada, que é a navegação
//   falando/silencio  ø6 de 16   a presença no roster, ao lado do apelido
//   comando          ~11 de 16   a tecla, dentro de uma frase de texto
//
// Triângulo cheio pesa mais que a letra ao lado se for desenhado na altura de
// caixa alta (0,698em na Plex Mono), então nenhum deles chega lá. O ⌘ chega,
// porque ele é lido como letra: 11,4 de 16 unidades, contorno incluído, é
// 0,71em — a altura de caixa alta, que é onde ele tem que estar para não
// parecer um símbolo colado numa frase.
//
// ---- linha de base ----
//
// `vertical-align: -0.2em` (em `base.css`) põe o centro da caixa a 0,3em da
// linha de base — entre o centro da altura-x (0,258em) e o da caixa alta
// (0,349em) da Plex Mono. Nos seis lugares onde eles aparecem a entrelinha é
// 16px e a caixa cabe dentro da haste da linha, então nenhuma linha cresce e
// nenhum botão muda de altura.
//
// ---- nome acessível ----
//
// Decoração ao lado de um rótulo de verdade não ganha nome: um triângulo que
// vem antes do nome da sala de voz é forma, e anunciar "triângulo apontando para
// baixo" antes de cada linha é ruído. Onde o glifo é o **conteúdo** do
// elemento, o nome é obrigatório — os dois botões da busca não têm mais nada
// dentro, e o ⌘ é a única coisa numa frase que diz qual tecla apertar. Os dois
// botões trazem o nome no `aria-label` do próprio botão; o ⌘ traz o seu aqui.

"use strict";

const SVG = "http://www.w3.org/2000/svg";

/**
 * Cada glifo é uma lista de formas, e cada forma é `[tag, atributos]`.
 *
 * `fill` não aparece: ele vem de `.glifo` em `base.css`, e é `currentColor`, de
 * modo que toda regra que já pintava o caractere continua pintando o desenho —
 * hoje, a cor herdada do `.botao-fantasma` nos dois botões da busca. As figuras
 * vazadas trazem `fill="none"` como atributo de apresentação, que vence a
 * herança sem vencer regra nenhuma.
 *
 * Quatro dos seis estão sem consumidor desde que a tela de sessão passou a
 * seguir o comp v2: lá a sala de voz aberta é uma borda de 2px e não um triângulo, e
 * quem fala é uma borda mais um fundo e não uma bolinha. Ficam desenhados
 * porque o que os justifica é a face, não a tela — a Plex Mono continua sem
 * eles, e a próxima tela que precisar de um triângulo não deve inventar o seu.
 */
const GLIFOS = {
  // ▸ e ◂ — as formas *small*. Aresta de 7, altura de 5, centradas em (8,8).
  avancar: [["path", { d: "M5.5 4.5L10.5 8L5.5 11.5Z" }]],
  recuar: [["path", { d: "M10.5 4.5L5.5 8L10.5 11.5Z" }]],

  // Aparência: um quadrado com a metade esquerda cheia, que é o desenho
  // universal de contraste. Da comp da 0.9.0.
  aparencia: [
    [
      "rect",
      {
        x: "2.5", y: "2.5", width: "11", height: "11",
        fill: "none", stroke: "currentColor", "stroke-width": "1.3",
      },
    ],
    ["rect", { x: "3.2", y: "3.2", width: "4.8", height: "9.6" }],
  ],

  // O botão de desligar: arco aberto em cima com uma haste subindo por dentro
  // dele. Desenho da comp da 0.9.0, e é o gesto que todo aparelho do mundo já
  // usa para «pare de fazer o que está fazendo» — aqui, sair da sala de voz.
  //
  // O arco abre exatamente onde a haste passa, e é isso que o faz ler como
  // desligar em vez de como um relógio.
  sair: [
    [
      "path",
      {
        d: "M4.8 4.8A4.5 4.5 0 1 0 11.2 4.8",
        fill: "none",
        stroke: "currentColor",
        "stroke-width": "1.3",
      },
    ],
    [
      "path",
      { d: "M8 2.2v5.4", fill: "none", stroke: "currentColor", "stroke-width": "1.3" },
    ],
  ],

  // ▼ e ▶ — o mesmo triângulo girado 90°, aresta de 9 e altura de 6.
  aberto: [["path", { d: "M3.5 5L12.5 5L8 11Z" }]],
  fechado: [["path", { d: "M5 3.5L11 8L5 12.5Z" }]],

  // ● e ○ — mesma pegada de 6 unidades nos dois, para que trocar de estado não
  // pareça trocar de tamanho: o vazado tem raio 2,3 e traço 1,4, e 2,3 + 0,7 é
  // exatamente o raio 3 do cheio.
  falando: [["circle", { cx: "8", cy: "8", r: "3" }]],
  silencio: [
    [
      "circle",
      { cx: "8", cy: "8", r: "2.3", fill: "none", stroke: "currentColor", "stroke-width": "1.4" },
    ],
  ],

  // ⌘ — o quadrado central e quatro laços de raio 1,7 nos cantos. Contorno, e
  // não preenchimento: a forma é feita de linha, e cheia ela vira um borrão.
  comando: [
    [
      "path",
      {
        d:
          "M6.3 6.3h3.4v3.4H6.3z" +
          "M6.3 6.3V4.6a1.7 1.7 0 1 0-1.7 1.7h1.7z" +
          "M9.7 6.3h1.7a1.7 1.7 0 1 0-1.7-1.7v1.7z" +
          "M9.7 9.7v1.7a1.7 1.7 0 1 0 1.7-1.7H9.7z" +
          "M6.3 9.7H4.6a1.7 1.7 0 1 0 1.7 1.7V9.7z",
        fill: "none",
        stroke: "currentColor",
        "stroke-width": "1.2",
      },
    ],
  ],

  // ---- os oito do comp v3 ----
  //
  // Todos medidos antes de desenhados: li o `cmap` de
  // `fontes/ibm-plex-mono-400.woff2` com `fontTools` e nenhum dos oito está lá,
  // entre os 499 pontos de código que a face tem. O método foi conferido no
  // mesmo teste com três controles — `⌘` e `●` deram ausentes, coerente com
  // estarem proibidos aqui, e `█` deu presente, coerente com as barras de
  // blocos o usarem. Ver `.superpowers/sdd/comp-inventario-v3.md` §5.

  // ⌕ — a lupa da busca: aro e cabo, sem preenchimento.
  buscar: [
    ["circle", { cx: "7", cy: "7", r: "3.6", fill: "none", stroke: "currentColor", "stroke-width": "1.3" }],
    ["path", { d: "M9.7 9.7L13 13", stroke: "currentColor", "stroke-width": "1.3", fill: "none" }],
  ],

  // ⚙ — a engrenagem que abre a configuração. Aro, furo e seis dentes retos:
  // dente curvo em 16px vira mancha, e a folha de marca não usa curva decorativa.
  engrenagem: [
    ["circle", { cx: "8", cy: "8", r: "3.1", fill: "none", stroke: "currentColor", "stroke-width": "1.3" }],
    [
      "path",
      {
        d:
          "M8 1.9v2.1M8 12v2.1M14.1 8h-2.1M4 8H1.9" +
          "M12.3 3.7l-1.5 1.5M5.2 10.8l-1.5 1.5" +
          "M12.3 12.3l-1.5-1.5M5.2 5.2L3.7 3.7",
        stroke: "currentColor",
        "stroke-width": "1.3",
        fill: "none",
      },
    ],
  ],

  // ▤ — voltar aos canais. O quadrado com preenchimento horizontal do Unicode é
  // literalmente uma lista, que é para onde o botão leva.
  linhas: [
    ["rect", { x: "2.5", y: "3", width: "11", height: "10", fill: "none", stroke: "currentColor", "stroke-width": "1.3" }],
    ["path", { d: "M4.8 6h6.4M4.8 8h6.4M4.8 10h6.4", stroke: "currentColor", "stroke-width": "1.1", fill: "none" }],
  ],

  // ⏻ — sair da sala. O símbolo de energia: haste vertical e aro aberto no
  // topo. O arco é longo e no sentido de baixo, para a abertura ficar em cima.
  desligar: [
    ["path", { d: "M4.8 4.8A4.5 4.5 0 1 0 11.2 4.8", fill: "none", stroke: "currentColor", "stroke-width": "1.3" }],
    ["path", { d: "M8 2.2v5.4", stroke: "currentColor", "stroke-width": "1.3", fill: "none" }],
  ],

  // ◍ — a seção de áudio. Aro com a faixa vertical cheia do próprio glifo, que
  // ao lado de "MICROFONE E SOM" lê como nível.
  audio: [
    ["circle", { cx: "8", cy: "8", r: "5", fill: "none", stroke: "currentColor", "stroke-width": "1.3" }],
    ["rect", { x: "6.6", y: "4.4", width: "2.8", height: "7.2" }],
  ],

  // ⌨ — a seção de atalhos. Moldura e teclas, canto reto como tudo aqui.
  teclado: [
    ["rect", { x: "1.8", y: "4.5", width: "12.4", height: "7", fill: "none", stroke: "currentColor", "stroke-width": "1.2" }],
    [
      "path",
      {
        d: "M4 6.7h1.2M6.6 6.7h1.2M9.2 6.7h1.2M11.8 6.7h1.2M4 9.3h8.9",
        stroke: "currentColor",
        "stroke-width": "1.1",
        fill: "none",
      },
    ],
  ],

  // ◧ — a seção de aparência. Metade cheia, metade vazia: é o próprio glifo do
  // Unicode e é a melhor imagem possível de "tema".
  aparencia: [
    ["rect", { x: "2.5", y: "2.5", width: "11", height: "11", fill: "none", stroke: "currentColor", "stroke-width": "1.3" }],
    ["rect", { x: "3.2", y: "3.2", width: "4.8", height: "9.6" }],
  ],

  // ⤓ — a seção de atualização. Seta para baixo pousando sobre uma linha: é a
  // imagem de instalar, e não a de baixar. A diferença importa aqui, porque o
  // que este botão leva a fazer não é guardar um arquivo — é trocar o programa
  // que está rodando e reabri-lo.
  //
  // Desenhada e não digitada pela razão dos outros oito: `U+2913` não está entre
  // os pontos de código da face embarcada, e digitá-lo poria a monoespaçada do
  // sistema no meio da coluna de seções.
  atualizar: [
    ["path", { d: "M8 2.2v6.6", stroke: "currentColor", "stroke-width": "1.3", fill: "none" }],
    [
      "path",
      { d: "M4.9 5.9L8 9L11.1 5.9", stroke: "currentColor", "stroke-width": "1.3", fill: "none" },
    ],
    ["path", { d: "M3 12.4h10", stroke: "currentColor", "stroke-width": "1.3", fill: "none" }],
  ],

  // ⚿ — a seção de identidade. Chave de palhetão quadrado: anel, haste e dois
  // dentes. É a chave Ed25519 do ADR 0004, e não um cadeado — o produto não
  // tranca nada aqui, ele prova quem é.
  chave: [
    ["circle", { cx: "5.2", cy: "5.2", r: "2.6", fill: "none", stroke: "currentColor", "stroke-width": "1.3" }],
    ["path", { d: "M7 7l5.4 5.4M10.2 10.2l-1.6 1.6M12.4 12.4l-1.6 1.6", stroke: "currentColor", "stroke-width": "1.3", fill: "none" }],
  ],

  // ▤ — um arquivo pendurado numa mensagem. ADR 0027. Uma folha com o canto
  // dobrado, e não um clipe de papel: o clipe é o desenho de «anexar», que é um
  // verbo, e o que este glifo marca é um substantivo — o arquivo que está ali.
  // O canto dobrado é o que distingue uma folha de um retângulo qualquer.
  anexo: [
    [
      "path",
      {
        d: "M4 2.5h5l3 3v8h-8Z",
        fill: "none",
        stroke: "currentColor",
        "stroke-width": "1.3",
      },
    ],
    [
      "path",
      { d: "M9 2.5v3h3", fill: "none", stroke: "currentColor", "stroke-width": "1.3" },
    ],
  ],
};

/**
 * O desenho de um glifo, pronto para entrar na página.
 *
 * `rotulo` só quando o glifo é a única coisa que o elemento diz. Sem ele o
 * desenho sai `aria-hidden`, que é o certo para decoração ao lado de um rótulo.
 *
 * Uma tela nova usa esta função. Ela não inventa o seu próprio triângulo: seis
 * desenhos ligeiramente diferentes da mesma seta é como uma interface deixa de
 * parecer uma interface.
 */
function glifo(nome, rotulo) {
  const formas = GLIFOS[nome];
  if (!formas) throw new Error(`glifo desconhecido: ${nome}`);

  const desenho = document.createElementNS(SVG, "svg");
  desenho.setAttribute("class", "glifo");
  desenho.setAttribute("viewBox", "0 0 16 16");
  for (const [tag, atributos] of formas) {
    const forma = document.createElementNS(SVG, tag);
    for (const [atributo, valor] of Object.entries(atributos)) forma.setAttribute(atributo, valor);
    desenho.append(forma);
  }

  if (rotulo) {
    desenho.setAttribute("role", "img");
    desenho.setAttribute("aria-label", rotulo);
  } else {
    desenho.setAttribute("aria-hidden", "true");
  }
  return desenho;
}

// Os glifos que estão na marcação, e não numa lista desenhada em tempo de
// execução. `data-glifo` em vez do desenho escrito à mão dentro do `index.html`
// porque duas cópias de um contorno é uma que vai ficar para trás — a mesma
// razão pela qual `tests/marca.rs` guarda uma cópia só de cada SVG da marca.
//
// Roda no fim do `<body>`, com a página inteira já analisada, e antes de
// qualquer pintura: nenhum quadro chega a mostrar o buraco.
for (const alvo of document.querySelectorAll("[data-glifo]")) {
  alvo.replaceChildren(glifo(alvo.dataset.glifo));
}
