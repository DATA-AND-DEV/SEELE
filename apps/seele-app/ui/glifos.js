// SEELE · Entry Plug — os seis glifos que a face de dados não tem.
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
//   aberto/fechado   9×6 de 16   o Cage aberto ou fechado, que é a navegação
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
// Decoração ao lado de um rótulo de verdade não ganha nome: o triângulo do
// Cage vem antes do nome do Cage, a bolinha vem antes do apelido, e anunciar
// "triângulo apontando para baixo" antes de cada linha é ruído. Onde o glifo é
// o **conteúdo** do elemento, o nome é obrigatório — os dois botões da busca
// não têm mais nada dentro, e o ⌘ é a única coisa numa frase que diz qual
// tecla apertar. Os dois botões trazem o nome no `aria-label` do próprio
// botão; o ⌘ traz o seu aqui.

"use strict";

const SVG = "http://www.w3.org/2000/svg";

/**
 * Cada glifo é uma lista de formas, e cada forma é `[tag, atributos]`.
 *
 * `fill` não aparece: ele vem de `.glifo` em `base.css`, e é `currentColor`, de
 * modo que toda regra que já pintava o caractere continua pintando o desenho —
 * `.lista .piloto.falando .presenca`, `.compor .prompt`, a cor herdada do
 * `.botao-fantasma`. As figuras vazadas trazem `fill="none"` como atributo de
 * apresentação, que vence a herança sem vencer regra nenhuma.
 */
const GLIFOS = {
  // ▸ e ◂ — as formas *small*. Aresta de 7, altura de 5, centradas em (8,8).
  avancar: [["path", { d: "M5.5 4.5L10.5 8L5.5 11.5Z" }]],
  recuar: [["path", { d: "M10.5 4.5L5.5 8L10.5 11.5Z" }]],

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
