# 0049 — Um MOD deixa de rodar na janela do produto

Status: **rascunho** — desenho, não decisão. Nenhuma linha de produção foi
tocada para escrevê-lo.
Data: 2026-09-18
Sobre o commit `c951c63`.

> **Este documento revisa o [ADR 0045](0045-mods-o-produto-base-tem-regras-e-um-mod-nao.md),
> que é contrato com quem escreve MOD.** Ele não pode ser aplicado sem migração
> dos três MODs oficiais e sem um número de versão que diga que o contrato
> mudou. A escolha do mecanismo de isolamento **não está fechada**: ela depende
> de um protótipo em macOS e Windows, e esta página diz o que o protótipo tem de
> responder.

## O que o 0045 prometeu, e o que ele não pode cumprir

O ADR 0045 diz que o produto base tem regras e um MOD não: um MOD alcança a
janela inteira, e a confiança vem do aceite informado, não de uma jaula.

A promessa vizinha, que o plano de isolamento de 18/09 cobra, é outra: **o que
um MOD faz vale enquanto você está naquele servidor, e some quando você sai.**

As duas não cabem juntas, e o plano diz por quê em uma frase que eu não melhoro:
*«manter JS arbitrário no contexto principal e prometer ausência de efeitos fora
do servidor são objetivos incompatíveis»*.

O que hoje torna a segunda promessa indefensável, medido no código:

- **o MOD compartilha tudo.** `withGlobalTauri: true` põe o objeto do Tauri no
  escopo global, e o script do MOD é carregado como `<script type="module">` na
  mesma página. `Object.freeze(SeeleMods)` congela **aquele objeto**, e não os
  caminhos por onde se chega ao mesmo lugar;
- **o descarte é cooperativo.** A contenção que entrou na etapa 1 desta volta
  desmonta o que o MOD registrou *por meio da API* e dispara `seele-mod-unload`.
  Um MOD que não o implemente deixa `setInterval`, ouvintes de evento, regras de
  CSS, áudio tocando e promessas atrasadas. Isso não é defeito daquele MOD: é o
  que «rodar na página» significa;
- **a aparência é global por construção.** O ESTILO escreve os tokens em
  `document.documentElement`. Não há onde escrevê-los que não seja global,
  porque a sessão não tem um contêiner próprio.

## A forma proposta

**Um MOD deixa de rodar na janela do produto.** Ele passa a rodar num contexto
descartável, e contribui para a interface por uma API, em vez de escrever nela.

Três consequências que decorrem disso, e não são negociáveis dentro dela:

1. **Destruir o contexto destrói o MOD.** Temporizadores, ouvintes, áudio e
   promessas morrem com ele, sem depender de o MOD cooperar. É a única forma de
   a promessa «some quando você sai» ser uma garantia em vez de um pedido.
2. **A aparência vira uma camada declarativa sobre o contêiner da sessão.** O
   tema deixa de ser escrito no documento e passa a ser um conjunto de valores
   que o produto aplica — e remove inteiro ao desmontar, reaparecendo a
   preferência de quem usa, sem fixar uma cor padrão por cima dela.
3. **O que um MOD pode desenhar passa a ser o que a API oferece.** Regiões,
   tema, apresentação de canais e pessoas. Ampliar o alcance passa a ser uma
   mudança da API — escrita, revisada, versionada — em vez de um MOD descobrir
   que consegue.

## A escolha que um protótipo tem de fechar

Duas famílias, e o plano de 18/09 já descarta o meio-termo: *«um iframe de mesma
origem ou Shadow DOM, isoladamente, não oferece essa garantia»*.

| | **Worker** | **Webview própria** |
|---|---|---|
| Alcance ao DOM do produto | nenhum, por construção | nenhum, por origem |
| Desenho | só por mensagem ao produto | própria, dentro da região |
| Custo por MOD | baixo | uma webview por MOD |
| `mod://` e CSP | some: o código vai por mensagem | continua, com origem própria |
| Depuração para quem escreve MOD | pior: sem DOM, sem inspetor | melhor |
| Risco de plataforma | baixo | **desconhecido** no Windows e no macOS |

**O que o protótipo tem de responder**, e por que cada pergunta existe:

- uma webview aninhada por MOD é viável nos dois sistemas, com o custo de
  memória de três MODs ao mesmo tempo? O `windows-2022` nunca rodou (pendência
  38), e medir isto no Mac só responde metade;
- destruir o contexto libera de fato o que o MOD alocou, ou sobra processo?
- o atraso de uma mensagem entre contextos é aceitável para o que os MODs
  fazem hoje — a MESA redesenha a cada movimento de peça;
- o desenho que o ESTILO faz cabe numa API de tema, ou ele precisa de mais do
  que uma camada declarativa consegue?

**Minha inclinação, e não a decisão:** worker para a lógica, e regiões
declarativas para o desenho. Ele é o que dá a garantia mais forte com o menor
risco de plataforma, e empurra o desenho para uma API que precisa existir de
qualquer forma. O custo está em quem escreve MOD — e é um custo real, que a
migração dos três oficiais vai medir antes de qualquer outro autor pagá-lo.

## O que isto quebra, dito antes

**Os três MODs oficiais param de funcionar como estão escritos.** MESA, ESTILO e
PERFIS desenham direto no documento. Nenhum deles sobrevive à mudança sem ser
reescrito na API nova, e é por isso que este ADR e a migração dos três são a
mesma entrega — publicar a mudança sem eles seria publicar um produto cujos
MODs oficiais não abrem.

**Todo MOD de terceiro quebra**, se existir algum. A API de MODs está em `2`
desde a v0.11.0, e um contrato novo é `3` — com o `api-too-new` do indexador
recusando pacote velho para build novo, que é o mecanismo que já existe.

**A promessa do 0045 fica menor, e isso é o ponto.** «Um MOD não tem regras»
passa a ser «um MOD não tem regras **dentro do que a API alcança**». É menos
liberdade e mais promessa cumprida, e a troca é deliberada: a liberdade atual é
o que impede o produto de garantir o que ele já diz garantir.

## O que este ADR não decide

- **o mecanismo**, que depende do protótipo acima;
- **o prazo**, que depende de quando os três MODs oficiais puderem ser
  reescritos;
- **o que fazer com quem já instalou um MOD de terceiro.** O catálogo é
  append-only e as versões antigas continuam no ar; um build novo passa a
  recusá-las por API, com a frase que o indexador já tem. Não há revogação aqui.
