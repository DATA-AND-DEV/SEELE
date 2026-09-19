# Notas da versão — v0.12.0

> **Esta página é o texto para o corpo do release.** Ela fica no repositório
> porque a v0.12.0 ainda **não foi publicada**: a tag e a publicação são passo
> manual de quem opera. O passo a passo está em
> `docs/mods-api-propria/runbook-publicacao-v0.12.0.md`.

_As mudanças de `v0.11.2` até aqui, em **dois dias** e 51 commits. O número sai
do `git log` na hora de marcar a tag; se este texto for publicado depois, refaça
a conta em vez de copiar a de cima._

---

## ⚠️ Esta versão não fala com a anterior

**Leia antes de atualizar uma máquina só.**

A `v0.11.2` publicada fala **protocolo 6**. A v0.12.0 fala **protocolo 7**.

Conferido lendo o commit que gerou a release publicada (`e90bcc16`), e não
suposto: `PROTOCOL_VERSION = 6` lá, `= 7` aqui.

A janela de compatibilidade é de uma versão, e ela **não resolve este caso**: um
par v0.11.2 recebe o carimbo 7, não o reconhece, e fecha com `PeerTooNew` antes
de ler o corpo. A janela serve para o lado novo aceitar um carimbo velho — e o
lado velho não tem como aceitar um carimbo que ainda não existia quando ele foi
compilado.

**Atualize as duas pontas.** Quem hospeda e quem entra.

## Os MODs mudaram de API, e a mudança é uma ruptura

`MOD_API_VERSION` passou de **2** para **3**, e a regra é igualdade: um MOD que
declara `api: 2` não roda neste build, e um que declara `api: 3` não roda nos
anteriores. Não há modo de compatibilidade, e é decisão: a API 3 **tirou**
coisa — um MOD de API 2 não é um MOD que pede menos, é um MOD que pede o que não
existe mais.

**O que isso quer dizer na prática:** ao atualizar, os MODs que você tem
instalados param de carregar até você instalar a versão 2.0.0 deles. As três
versões novas — ESTILO, PERFIS e MESA — são publicadas junto com esta release.

### Um MOD deixou de rodar dentro da janela

É a mudança de fundo, e o ADR 0049 a descreve inteira. Antes, o cliente de um
MOD era código rodando na mesma página que a sua conversa, com acesso ao DOM.
Agora ele roda num contexto QuickJS separado, onde **não existem** `document`,
`window`, `fetch`, `localStorage`, `indexedDB` nem o global do Tauri — e não
existem porque ninguém os ligou, e não porque alguém os apagou no começo do
arquivo. A diferença importa: o segundo caso se contorna.

O MOD **declara** o que quer mostrar, e quem monta é o produto, com
`createElement` e `textContent`. Nunca com marcação vinda de fora.

O primeiro desenho desta API usava um `Worker` de `blob:`, e ele foi **reprovado
por medição**: um worker de `blob:` herda a origem de quem o criou, e o que um
MOD gravou em `indexedDB` sobreviveu ao encerramento do aplicativo e reapareceu
na entrada seguinte. `terminate()` mata o contexto e não toca no armazenamento
da origem. Hoje há um executor só.

### O que um MOD passou a poder fazer

A API 2 mostrava texto. A API 3 tem campos que se editam sem perder o foco,
escolhas, botões, desenho declarado com arraste, som e imagem que vêm do pacote
ou do servidor do próprio MOD, e arquivo escolhido por quem usa — pelo seletor
do sistema, aberto pelo produto, com os bytes lidos em pedaços.

Duas superfícies alcançam a janela além da região do MOD, e as duas são
estreitas de propósito:

- **o tema da sessão** — seis cores, densidade, família de tipo, arredondamento
  e brilho. Vale só naquela sessão e sai com ela, sem tocar em preferência
  nenhuma sua. O produto confere contraste mínimo de 4,5:1 e recusa o que não
  alcança;
- **o cartão na lista de pessoas** — um MOD de perfis pode pôr retrato, nome
  escolhido e pronome ao lado de quem está na conversa. Ele **declara**, com uma
  gramática menor que a da região — nada que receba foco ou clique —, e quem
  desenha é o produto.

### Cantos arredondados, quando quem administra quiser

O SEELE abre em canto reto e sem sombra, e continua abrindo. O que mudou é que
um tema de servidor pode levantar os dois, dentro de limites que o produto
confere: arredondamento inteiro de 0 a 24, e brilho ligado ou desligado com a
sombra montada pelo produto. Vale só na sessão daquele servidor.

## Uma dependência a dizer em voz alta

O indexador de MODs — `mods.seele.app.br` — é servido por Cloudflare Pages.
**Quem serve os bytes vê quem os pediu**, e num produto cujo argumento é não ter
serviço no meio isso merece ser dito e não descoberto.

Não há como zerar isso com hospedagem estática. O que foi feito: **Web Analytics
e Logpush desligados**, e nada de análise no projeto. Assim os registros ficam
sendo os que a Cloudflare guarda para si, em vez de os que pedimos que ela
guardasse para nós. A diferença é pequena e é real.

O catálogo é estático e a busca acontece na sua máquina: o indexador aprende que
alguém baixou o catálogo, e não o que alguém procurou.

## O que não mudou

A chave que confere os MODs é a mesma dos releases anteriores
(`32D58C50C0AB34E8`), e ela é separada da que autoriza atualizar o programa. Uma
chave que atesta duas coisas deixa as duas se passarem uma pela outra.
