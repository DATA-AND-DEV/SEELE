# 0049 — Um MOD deixa de rodar na janela do produto

Status: **aceito** — as três decisões abertas foram respondidas por quem
responde pelo projeto em 2026-09-18, e estão registradas abaixo com o que cada
uma custa.
Data: 2026-09-18
Sobre o commit `c951c63`.

> **Este documento revisa o [ADR 0045](0045-mods-o-produto-base-tem-regras-e-um-mod-nao.md),
> que é contrato com quem escreve MOD.**
>
> **Decidido:** worker para a lógica e regiões declarativas para o desenho;
> ruptura limpa na **API 3**, sem caminho de compatibilidade; e a reescrita dos
> três MODs oficiais fica com o Codex, enquanto este lado entrega a API, o
> runtime e o guia.

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

**Decidido: worker para a lógica, regiões declarativas para o desenho.** Ele dá
a garantia mais forte com o menor risco de plataforma, e empurra o desenho para
uma API que precisa existir de qualquer forma.

E a escolha **dispensa o protótipo de webview aninhada**, que era o que trazia
risco de plataforma desconhecido: `Worker` é a mesma coisa nos três sistemas, e
`terminate()` é o mesmo contrato. As três perguntas que sobram — tempo de
mensagem, liberação de memória, e se o desenho do ESTILO cabe numa camada
declarativa — se respondem escrevendo, e não medindo webview.

**Como o código chega ao worker, e por que não por `mod://`.** Um `Worker` não
aceita URL de outra origem, e `mod://localhost` é outra origem. As saídas
seriam alargar `connect-src` para buscar o texto por `fetch`, ou pedi-lo pela
ponte que já existe. A segunda é melhor e não custa nada: o texto vem por um
comando, que é onde a conferência de hash **já mora**, e vira um `Blob` de mesma
origem. A CSP ganha `worker-src blob:` e mais nada.

O custo está em quem escreve MOD — e é real. Sem DOM, sem inspetor, e tudo o
que se desenha passa por uma API que precisa existir primeiro.

## O que isto quebra, dito antes

**Os três MODs oficiais param de funcionar como estão escritos.** MESA, ESTILO e
PERFIS desenham direto no documento. Nenhum deles sobrevive à mudança sem ser
reescrito na API nova, e é por isso que este ADR e a migração dos três são a
mesma entrega — publicar a mudança sem eles seria publicar um produto cujos
MODs oficiais não abrem.

**Todo MOD de terceiro quebra**, se existir algum. A API de MODs está em `2`
desde a v0.11.0, e um contrato novo é `3` — com o `api-too-new` do indexador
recusando pacote velho para build novo, que é o mecanismo que já existe.

**Decidido: ruptura limpa, sem caminho de compatibilidade.** A alternativa era
deixar MOD de API 2 rodando do jeito antigo, na janela, com aviso — e ela foi
recusada pela razão certa: enquanto houvesse um MOD antigo carregado, a promessa
«some quando você sai» continuaria falsa, e o produto passaria a prometer duas
coisas diferentes ao mesmo tempo. Uma data só, e ela é honesta.

**A promessa do 0045 fica menor, e isso é o ponto.** «Um MOD não tem regras»
passa a ser «um MOD não tem regras **dentro do que a API alcança**». É menos
liberdade e mais promessa cumprida, e a troca é deliberada: a liberdade atual é
o que impede o produto de garantir o que ele já diz garantir.

## Quem escreve o quê, e o risco que isso deixa

**Decidido:** este lado entrega a API, o runtime isolado e o guia de migração; a
reescrita de MESA, ESTILO e PERFIS fica com o Codex, que já trabalha nesses
repositórios.

**O risco que a divisão cria, dito por quem a está executando:** uma API que sai
sem nenhum uso real a provar é uma API que parece pronta. Foi assim que o
`ida_e_volta` ficou anos verde sem funcionar, e é o padrão que o `CLAUDE.md`
chama de «existir não é funcionar».

**Como ele é coberto sem pisar nos três:** um MOD de referência entra como
**vetor deste repositório** — não publicado, não oficial, escrito só para
exercitar a API de ponta a ponta na bancada. Ele desenha numa região, lê e
aplica tema, faz um pedido ao servidor e é descarregado, e a bancada afirma
sobre o que ele faz. Se a API não servir para ele, ela não serve para os três —
e isso aparece aqui, antes de o Codex começar.

## O que este ADR não decide

- **o prazo**, que depende de quando os três MODs oficiais puderem ser
  reescritos;
- **o que fazer com quem já instalou um MOD de terceiro.** O catálogo é
  append-only e as versões antigas continuam no ar; um build novo passa a
  recusá-las por API, com a frase que o indexador já tem. Não há revogação aqui.

## Emenda de 18/09/2026 — o que a medição sustenta, e o que não

A consequência 1 diz «destruir o contexto destrói o MOD». Uma sonda rodada no
aplicativo nativo — `apps/seele-app/testes/sonda-de-fronteira/`, resultados em
[`mods-api-propria/registro-de-execucao.md`](../mods-api-propria/registro-de-execucao.md)
— mediu o que essa frase alcança.

**Ela vale para a execução do próprio MOD.** O worker morre com a sessão, e com
ele o que rodava lá dentro.

**Para descendentes, não está medido.** A primeira leitura desta emenda dizia
que estava: um `Worker` filho parou 2,4 segundos depois de a sessão encerrar e
não voltou a bater. A medição não sustenta a conclusão — o processo do
aplicativo foi reiniciado entre as duas leituras, e a morte do processo explica
o mesmo resultado. Separar as duas coisas exige encerrar a sessão **sem**
encerrar o processo, e isso, na bancada de hoje, precisa de um clique na
janela.

A especificação do HTML manda que encerrar um worker encerre os workers que ele
possui, e é o que se espera; o que falta é a observação no motor. Está escrito
como pendência em
[`mods-api-propria/registro-de-execucao.md`](../mods-api-propria/registro-de-execucao.md),
e não como garantia.

**Ela não vale para armazenamento.** Um worker de `blob:` herda a origem de quem
o criou, e com ela `indexedDB` e `caches` do produto. O que o MOD gravou lá
**sobreviveu** ao encerramento do aplicativo e reapareceu na entrada seguinte.
`terminate()` mata o contexto e não toca no armazenamento da origem.

**E `BroadcastChannel` também é da origem**, então dois MODs podem conversar sem
passar pela API, e com a janela se alguém escutar.

Apagar esses nomes de dentro do prelúdio não é fronteira: o contrato de API
própria exige uma barreira **verificável**, e um `delete` no começo do arquivo
não é verificável — seja qual for o argumento sobre ordem de execução.

**A consequência para este ADR: o Worker de Blob não vai ser o executor.** A
marca relida depois de reiniciar basta para isso. O próximo experimento é
QuickJS nativo no cliente, mantendo esta janela e um renderer confiável; origem
isolada para Worker fica como alternativa se ele reprovar. A escolha não está
feita, e o que este ADR registra desde já é que a promessa publicada não pode
ser mais larga do que a medida.

O resto do 0049 continua de pé: um MOD não roda na janela do produto, o desenho
é declarado, e a ruptura com a API 2 é limpa. O que a medição move é **qual
contexto** executa a lógica, e não se ele é separado da janela.

## Emenda de 19/09/2026 — a gramática cresce onde um aviso a substituía

O 0049 deixou a gramática da região pequena de propósito: «cada forma nova é uma
decisão de API, em vez de um MOD descobrir que consegue». Esta emenda é a
decisão, e ela é tomada olhando para uma coisa concreta — o que os três MODs
oficiais passaram a **não conseguir fazer** depois da ruptura.

O ESTILO, migrado para a API 3, tinha esta linha na tela de quem o instalou:

> A API 3 oferece acento, fundo, texto e borda. Edição de tema, tipografia,
> espaçamento, arredondamento e brilho aguardam suporte do SEELE.

Um aviso de indisponibilidade no lugar de um comportamento é a forma mais barata
de não entregar uma coisa. A MESA virou um painel que lista o tabuleiro em vez
de mostrá-lo; o PERFIS, uma ficha que não se edita. Os três viraram telas de
leitura, e nenhum deles avisou que isso era o que ia acontecer: eles fizeram o
que a API permitia.

### O que entra

**Quatro formas**, todas declaradas e montadas pelo produto, como as cinco de
antes:

- `campo` e `escolha` — o que a pessoa escreve e o que ela seleciona. A escolha
  só deixa sair um dos valores declarados, e é isso que faz uma paleta não
  precisar de conferência do outro lado;
- `botao` — o que ela aperta;
- `tela` — onde se arrasta e se desenha. Ela aceita **figuras declaradas com
  chave**, e o evento de arraste diz qual foi pega. Sem isso, arrastar uma peça
  daria ao MOD um par de coordenadas e caberia a ele refazer o acerto que o
  produto acabou de fazer para pintar — pior, porque ele não sabe a ordem em que
  as figuras ficaram na tela;
- `midia` — som e imagem, **do pacote** ou **da metade de servidor do MOD**. Não
  há uma terceira origem, e a que falta é justamente a que faria a janela de
  quem conversa buscar bytes na rede de um estranho.

**Um caminho de volta**: `SeeleUI.aoEvento`. A janela fala com o MOD sem que ele
tenha perguntado. Um pedido tem número e resposta; um evento não tem nem um nem
outro, porque quem digita não espera o MOD confirmar que recebeu a tecla.

**Dois tokens de tema e uma medida**: `painel`, `apagado` e `densidade`. Os dois
primeiros existiam no produto e faltavam na API — o MOD guardava seis cores no
servidor e conseguia aplicar quatro. A densidade é **escolha, e não número**:
uma cor é contínua e o produto confere o contraste dela; um espaçamento não tem
como ser conferido assim, e um MOD que pedisse `0px` deixaria a sessão ilegível
sem violar regra nenhuma. Duas densidades, com os números do produto, dão a
escolha sem dar a régua.

### Tipografia: escolha, e recurso de sessão

O ESTILO guarda `mono` e `sans`. O medo que os deixava de fora era certo: a
escala de tipo deste produto é medida e afirmada em `tokens.css` — tamanho,
entrelinha e contraste andam juntos —, e uma família **livre** move os três de
uma vez sem nada conferir o resultado.

O que resolve não é ausência, é a mesma forma que resolveu a densidade: **o MOD
nomeia, o produto fornece**. `tokens.css` declara duas pilhas — `--seele-pilha-mono`
e `--seele-pilha-sans` —, e `fonte` aceita um desses dois nomes. As duas são as
que o produto já usa e já mediu, e nenhuma família de fora entra.

E ela é **recurso de sessão**, como as cores: escrita no contêiner da sessão,
tirada quando a sessão acaba, sem tocar em preferência nenhuma de quem usa.

### Arredondamento e brilho: emenda de 19/09

`--seele-raio` e `--seele-sombra` já existiam em `tokens.css`, valendo `0` e
`none`, e três folhas já os liam. O que faltava não era o token: era alguém
poder escrevê-los.

Uma versão anterior desta emenda os **recusou pelo nome**, citando
`docs/marca.md`. A citação estava errada, e vale registrar por que: aquele
documento diz de si mesmo, na terceira linha, que governa «a única imagem que o
produto tem de si mesmo» e que **nada nele alcança a estética**. A regra 1 de lá
é sobre o símbolo. Quem governa o recuo da interface é
`specs/07-estetica.md` — e é lá que a exceção foi escrita, não aqui.

A forma é o que importa:

- **`arredondamento` é inteiro de 0 a 24**, conferido pelo produto, e é o
  produto que escreve o `px`. Um raio livre aceita `9999`, que transforma cada
  painel num comprimido, e aceita `calc(...)` com o que vier dentro;
- **`brilho` é sim ou não**, e a sombra que ele liga é montada pelo produto a
  partir de um token do produto. Aceitar a sombra escrita pelo MOD faria a API
  de tema virar a porta de entrada de qualquer `box-shadow`, inclusive um que
  desenhe fora do painel e cubra a tela;
- os dois são **recurso de sessão**, como as cores e a tipografia, e o mesmo
  desempate por nome vale: dois MODs pedindo arredondamentos diferentes é a
  mesma pergunta sem resposta que dois pedindo o acento.

### O cartão na lista de pessoas: emenda de 19/09

Esta é a **única superfície de um MOD fora da região dele**, e a primeira coisa
a dizer é o que ela não é: não é o MOD desenhando na janela. O que chega é a
**mesma declaração da região**, montada pelo **mesmo renderer** — `planejar`,
`reconciliar`, `elemento`, `createElement`, `textContent`. Nada de marcação,
nada de estilo escrito por terceiro, nada de nó alcançável de fora.

Ela existe porque a lista de pessoas é onde uma informação de MOD tem lugar
natural: o retrato e o nome que alguém escolheu num MOD de perfis são sobre
aquela pessoa, e mostrá-los só dentro de um painel ao lado é mostrá-los longe de
onde significam alguma coisa.

Três coisas a mantêm sendo do produto:

- **a gramática é menor.** `FORMAS_DO_CARTAO` exclui `campo`, `escolha`,
  `botao`, `arquivo` e `tela` — tudo o que recebe foco ou clique. A linha do
  roster já tem um botão do produto (o nome, que abre a moderação), e dividir a
  ordem de tabulação e a área de toque de um controle do produto com um
  terceiro é o tipo de coisa que ninguém consegue depurar depois. Uma forma
  recusada num cartão **é contada**, e o MOD recebe o número: ele precisa saber
  que pôs um botão onde botão não entra, em vez de vê-lo sumir;
- **os tetos são próprios, e menores.** 64 pessoas, 24 nós por cartão, 4 níveis
  de fundura, e mídia com contador e orçamento de bytes separados dos da
  região. Um cartão é por pessoa: os 512 nós da região valeriam vinte vezes
  numa lista de vinte, e a lateral inteira viraria o painel de um MOD. O que
  não couber em 24 nós é uma região, e a região já existe;
- **o dono é a região.** O cartão nasce dentro da `RegiaoDeMod` daquela
  instância e sai com ela — é a mesma `soltar` que para o som e solta os bytes,
  e ela alcança as raízes que estão na lista do produto e não sob a raiz dela.
  Um retrato de um MOD que não está mais de pé continuaria ao lado do nome de
  alguém, e ninguém teria como tirá-lo.

O tamanho, a tipografia, o espaçamento e a posição do cartão na linha são da
folha do produto. O MOD escolhe o que o cartão diz; o produto escolhe quanto ele
ocupa.

### Arquivo: escolha de quem usa, mediada pelo produto

A MESA carrega cenas e o PERFIS carrega retratos, e as duas coisas exigem que
alguém escolha um arquivo. Nenhum cliente do SEELE abre arquivo por conta de
terceiro — o 0027 é explícito —, e isso continua valendo: **o MOD não recebe o
disco**.

O que ele recebe é o que a pessoa escolheu, depois de o produto ter olhado:

- a forma `arquivo` é um **botão**, e o seletor atrás dele é o do sistema,
  aberto por este processo. Um MOD não abre seletor sozinho; ele desenha o
  botão e espera alguém apertar;
- o que volta pelo evento é um **identificador**, o tipo que os **bytes**
  provaram ser, e o tamanho. O caminho no disco não atravessa, e o nome do
  arquivo também não — ele é texto que outra pessoa escolheu;
- os bytes ficam no produto e saem por `SeeleUI.pedaco(arquivo, inicio)`, em
  pedaços de 64 KiB. Um MOD que só precisa do começo não paga pelo resto, e um
  arquivo de dez megabytes não passa pela fila de mensagens de uma vez;
- **cancelar é uma resposta**: fechar o seletor sem escolher entrega `null` ao
  MOD, e não silêncio;
- `SeeleUI.soltar(arquivo)` devolve a memória na hora, e a **saída da sessão
  solta os dela de qualquer jeito**. Quatro arquivos de pé por sessão, dez
  megabytes cada: um MOD que abre o seletor num laço esbarra no teto antes de
  encher a memória de quem está numa conversa;
- um identificador é de **quem o pediu**. Ele é pequeno e sequencial, e sem
  essa conferência um MOD que adivinhasse um número leria o arquivo de outro.

O número nunca se repete nesta janela. Reaproveitá-lo faria um pedido atrasado
ler o arquivo que ocupou o lugar do dele — o arquivo errado, sem nada dizer.

### O que não muda

O MOD continua fora da janela. Ele **declara** e o produto monta, com
`createElement` e `textContent`, nunca com marcação. Cada coisa que ele cria
nasce com dono, teto e descarte. O tipo de uma mídia vem dos **bytes**, nunca do
manifesto nem do caminho — a mesma regra do 0027, pela mesma razão: o manifesto
é texto que um terceiro escolheu.
