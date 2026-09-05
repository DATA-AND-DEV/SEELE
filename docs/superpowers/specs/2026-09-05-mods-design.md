# MODs: o produto base tem regras, e um MOD não

> Estado: desenho em revisão. Nada construído.
>
> Pedido do dono, nas palavras dele: «O sistema terá um indexador de MODs,
> tanto oficiais, quanto da comunidade. Esses MODs são por SERVIDOR. Os dados
> são persistidos no server, e quem entrar no server precisa baixar esses MODs.
> Ele também pode em seu server, habilitar ou desabilitar MODs, mesmo eles
> estando baixados.»
>
> E a frase que decidiu o desenho inteiro, três rodadas depois: **«o que
> construímos até agora tem regras. Mas essas regras não se aplicam aos MODs, e
> essa é a graça.»**

## A tese

Um MOD é **código de verdade**, escrito por terceiros, rodando nos dois lados,
com acesso real. Ele não é uma caixa de areia e não é um esquema declarativo:
é o modelo do Forge, e a analogia é do dono — jogo base com mecânicas, MODs que
complementam ou refazem essas mecânicas de outro jeito.

As garantias que este repositório escreveu — palheta congelada, os quatro
guardas do vermelho, «movimento é diagnóstico», retransmissão de terceiro fora
de escopo — **protegem o produto base. Um MOD está fora delas por decisão.**

A defesa não é técnica: é **código aberto obrigatório mais revisão nossa de
cada versão publicada**. Isso está escrito aqui como o que é — a única defesa
real — para que ninguém daqui a um ano ache que havia outra.

## O que este desenho supera

Cada uma some por escrito, com quem a substitui. Um ADR encontrado depois e lido
como vigente é pior que nenhum ADR.

| decisão | onde | o que acontece |
|---|---|---|
| «Um MOD não acompanha um servidor» | ADR 0029 | **revogada** — o MOD é do servidor, e quem entra baixa |
| «Nada de código arbitrário» | ADR 0029 | **revogada** — MOD é código |
| Lista fechada de capacidades (só cor em v1) | ADR 0029 | **revogada** — não há lista fechada |
| «Nenhuma capacidade dá a um MOD lugar para escrever» | ADR 0029 | **revogada** — o MOD tem estado no servidor |
| Indexador como catálogo estático sem revisão | ADR 0029 | **emendada** — revisão de código de cada versão |
| Cor da placa por lista fechada | ADR 0032 | **absorvida** pelo MOD; não se constrói separada |
| «Um servidor não repinta a janela de quem entra» | ADR 0032 | **revogada** |
| Retransmissão de terceiro fora de escopo (degrau 5) | ADR 0022 | **emendada** — permitida por MOD, com aviso duro |
| «Movimento é diagnóstico», exceção única | specs/07 | **emendada** — MOD anima livre |
| «Um app, um botão de atualizar» | ADR 0026 | **substituída** por seletor de versão |
| `marketplace de plugins` como não-objetivo | specs/00 | **desfeita** |

O ADR 0029 continua valendo em duas coisas, e elas sobrevivem porque não eram
sobre poder: **o esquema só cresce** (capacidade não sai depois de publicada) e
**instalação parcial silenciosa é o modo de falha a evitar** — recusar nomeando
os dois números é melhor que rodar pela metade.

## Dois subsistemas, e o segundo depende do primeiro

O pedido virou dois projetos de naturezas diferentes. Escrevê-los como um só
esconderia o mais caro atrás do mais fácil, que é exatamente a advertência que o
ADR 0032 fez sobre nome, cor e ícone.

- **Multi-versão** — o produto passa a ser hospedável e alcançável em qualquer
  versão já publicada. Mexe no instalador, no atualizador, no protocolo e no
  banco.
- **MODs** — manifesto, API congelada, carga no cliente, runtime no servidor,
  estado, indexador.

**A dependência é estreita e vale dizer onde ela começa.** MODs podem ser
construídos e usados sem multi-versão nenhuma — instalados de arquivo, num
servidor de uma versão só. O que **não** pode acontecer sem ela é o catálogo
abrir para a comunidade: o pedido é explícito em que «MODs são feitos pensados
numa versão específica», e sem multi-versão todo MOD publicado quebra na
atualização seguinte e o catálogo apodrece sozinho.

Por isso a ordem de entrega no fim deste documento constrói MODs primeiro e o
multi-versão antes do indexador — e não antes de tudo.

---

# Subsistema 1 — Multi-versão

## O achado que obriga a existir

Há uma janela de compatibilidade no protocolo hoje, e **ela corre para o lado
errado deste pedido**. `crates/seele-proto/src/version.rs`:

```
PROTOCOL_VERSION = 3
COMPATIBILITY_WINDOW = 1
```

O comentário do módulo diz o resultado com todas as letras: *«um cliente v3 não
alcança mais um servidor v2»* — o servidor faz `negotiate(3)`, vê 3 acima do que
ele fala, e recusa com `Incompatible`. Cliente velho alcança servidor novo;
cliente novo **não** alcança servidor velho.

E não é um `if` a corrigir. Uma linha acima está o motivo: **o `postcard` indexa
variante por posição e não é autodescritivo** — uma variante desconhecida não é
ignorada, *«ela desloca a leitura do fluxo para sempre»*. Falar duas versões do
protocolo significa **manter dois conjuntos de tipos vivos no código**, não
tolerar um campo a mais.

Some-se: as migrações são append-only e `no_migration_contains_a_down_step`
reprova qualquer `DROP`. **Não há migração reversa neste projeto**, então um
servidor velho contra um banco que um servidor novo migrou não tem volta.

## A decisão

**O app vira launcher.** Cliente e servidor são versionados juntos; ao conectar,
o cliente roda a versão daquele servidor. É o modelo do Minecraft, escolhido
com o custo na mesa duas vezes.

- **Uma versão do produto é uma unidade.** Cliente, servidor, protocolo e API de
  MOD sobem juntos e são identificados pelo mesmo número. Não há matriz.
- **Toda versão publicada continua hospedável e alcançável**, para sempre — com
  uma exceção nomeada abaixo.
- **Um diretório de dados por versão**, porque não há migração reversa. Trocar de
  versão para trás não converte banco: abre outro, como um mundo de Minecraft.
  Isto precisa estar na tela, não numa nota de release: **descer de versão não
  leva as conversas junto.**
- **Cada build é assinada** pela mesma chave `minisign` do ADR 0026, e a
  conferência acontece antes de qualquer arquivo ser tocado — a propriedade que
  aquele ADR já comprou («falha no meio da atualização não deixa meia
  instalação») vale igual aqui.
- **O `latest.json` do ADR 0026 vira um manifesto de versões.** Continua morando
  no release do GitHub, continua sem serviço a hospedar.

## A exceção nomeada: revogação

«Toda versão para sempre» tem um dia ruim, e ele é previsível: uma versão velha
com falha de segurança continua rodável. Foi escolhido guardar tudo **menos as
com falha grave**, e isso exige uma peça que hoje não existe:

- Uma **lista de revogação assinada**, dentro do manifesto de versões.
- Uma versão revogada **não é hospedável e não é alcançável**, e a recusa diz
  qual é o defeito e qual versão o corrige — nunca uma recusa genérica, que é o
  que `specs/02-protocolo.md` já exige de toda razão.
- A lista precisa alcançar máquinas já instaladas. **Esta é a parte cara**, e a
  honestidade é dizer que ela reintroduz uma consulta que o ADR 0026 tinha
  orgulho de não ter. A mitigação: a lista é conferida ao **hospedar** e ao
  **entrar**, que são atos, e nunca ao abrir o app.

## O que o ADR 0026 perde

O botão «atualizar» deixa de ser a única forma de mudar de versão, e o argumento
dele — *«num app de conversa, um binário trocado sem aviso é intrusivo»* —
sobrevive: o seletor de versão é ainda mais explícito que o botão. O que morre é
«um app, uma versão instalada». Passa a haver disco ocupado por versões
guardadas, e uma tela que diz quanto e permite apagar.

---

# Subsistema 2 — MODs

## O que é um MOD

Um diretório com três coisas:

```
mod.json          manifesto, chaves em inglês (ADR 0013)
cliente/          JavaScript que a janela carrega
servidor/         JavaScript que o servidor roda
```

As duas metades são opcionais: um MOD de cor não tem `servidor/`, um bot de chat
não tem `cliente/`.

**Identidade** é `autor/nome`, mais versão, mais **hash do conteúdo**. O hash é o
que o servidor anuncia e o que o cliente confere; o nome é conveniência. Isto vem
direto do ADR 0026, alternativa 5: *«TLS diz de qual servidor o arquivo veio, e
não quem o produziu.»*

O manifesto declara, no mínimo:

- **`api`** — um inteiro, a versão de API contra a qual foi escrito. Uma só.
- **`reach`** — o que este MOD alcança, para a tela de aceite mostrar antes de
  qualquer coisa acontecer. Ler mensagens do canal é diferente de receber só o
  que é dirigido a ele, e a pessoa vê a diferença.
- **`repo`** — o repositório público. Obrigatório.
- **`state`** — o esquema de dados do MOD e a versão dele.

## Por que JavaScript nos dois lados

Decidido com o argumento na mão, e o argumento não é elegância.

1. **Fonte é artefato.** Não há build para reproduzir, então a revisão lê
   exatamente o que roda, e o indexador continua servindo arquivo estático
   assinado em vez de virar serviço de build. Com artefato compilado,
   revisaríamos o repositório e a pessoa instalaria um binário — a revisão viraria
   teatro.
2. **A casca já é JS.** `apps/seele-app/ui/` são ~15 mil linhas de JavaScript sem
   framework por decisão (ADR 0019). A metade de cliente de um MOD é trabalho de
   DOM, e pôr Rust ali seria hostil.
3. **Um artefato, três plataformas.** Ninguém compila nada.
4. **O piso de entrada decide o tamanho do catálogo.** O ecossistema do Minecraft
   existe porque o piso era Java, que meio mundo já sabia. Exigir Rust não deixa
   o desenho mais puro: deixa o catálogo vazio.

**O que se perde, dito aqui e não descoberto depois:** não há checagem de tipo do
lado do servidor. Um erro de digitação chega em execução, na máquina de quem
hospeda. A contrapartida é a seção «Falha isolada», e ela não é polidez — é o
preço desta escolha.

**Pendência medida, não suposta.** Qual interpretador embutir no servidor é a
única decisão deste desenho que depende de um número que ninguém tem. Um spike
mede, antes de qualquer linha: tamanho de binário, memória por contexto, custo de
chamada, e a conta contra o orçamento de **1 vCPU / 512 MB** que
`xtask/src/check_deps.rs` protege ao proibir `seele-server` de depender de
`seele-core` e de `seele-audio`. A intuição de que «WASM é mais leve que JS»
provavelmente é falsa aqui, e o `CLAUDE.md` deste repositório tem uma linha para
exatamente isto: *«Três vezes uma hipótese confiante custou mais que a medida
teria custado.»*

## A API é uma fachada congelada, não uma projeção

Esta seção existe porque a primeira versão dela estava errada, e o dono achou o
erro: se a API fosse **gerada a partir de `seele-proto`**, renomear um campo lá
dentro repintaria a API e quebraria todo MOD.

**O conserto é inverter o guarda.** Ele deixa de perguntar «todo tipo do
protocolo chegou à API?» e passa a perguntar «todo nome da API ainda aponta para
alguma coisa?».

```
api/v1.json    "pessoa.papel"     ←  Person::role
               "pessoa.apelido"   ←  Person::nickname
```

- **Renomear por dentro fica livre.** O build fica vermelho e o conserto é o
  mapeamento, não os MODs. O nome que o MOD escreve nunca mudou.
- **Acrescentar ao protocolo não mexe na API.** Um tipo novo só aparece para MODs
  quando alguém decide de propósito, e essa decisão sobe a versão da API. Nada
  vaza por acidente.
- **Cada versão da API é um arquivo congelado**, e um teste reprova qualquer
  edição num arquivo já publicado. É a mesma disciplina das migrações — *«Append
  only once shipped»* mais `no_migration_contains_a_down_step` —, que hoje segura
  dez migrações, aplicada a um segundo lugar onde ela vale pelo mesmo motivo.

Isto é o que faz um MOD parecer nativo em vez de enxertado: ele manipula as
coisas do SEELE, com os nomes do glossário, e a garantia de que continuam as
mesmas é mecânica, não uma promessa de manutenção.

**O que a fachada honestamente não salva:** nome é barato de preservar,
comportamento não é. Se «expulsar» passar a significar outra coisa, nenhuma
fachada segura um MOD que dependia do significado antigo — e é para esse dia que
o subsistema 1 existe.

## Como o código entra na janela

`tauri.conf.json` tem `script-src 'self'`, e `ui/` é embutido no binário em tempo
de compilação. Não existe hoje caminho por onde um `.js` de terceiro entre na
página, e afrouxar para `unsafe-eval` seria abrir a porta mais larga do prédio
para resolver o problema mais estreito.

**A saída é um protocolo próprio do Tauri.** Os arquivos do MOD ficam em disco,
um handler os serve sob `mod://`, e a CSP ganha esse esquema — e só ele — em
`script-src`. Sem `unsafe-inline`, sem `unsafe-eval`, sem origem remota. O
carregamento continua sendo o navegador buscando um arquivo.

O guarda que substitui o que se perde: **nada é servido sob `mod://` cujo hash
não bata com o que o servidor declarou.**

`the_page_loads_only_files_that_are_shipped` (`frontend.rs:520`) precisa de uma
emenda de escopo: ele passa a valer sobre `self`, e `mod://` ganha o guarda
acima. A emenda é de alcance, não de rigor.

**O alcance de um MOD de cliente é a janela inteira**, por decisão. Ele não está
numa caixa de areia, e é isso que permite o MOD de layout e o de perfil.

## Estado no servidor

Migração **11** — a próxima, já que há dez.

- Tabela de MODs: identidade, versão, hash, habilitado, configuração.
- Um espaço de dados nomeado por MOD, com teto declarado, alcançado por uma API
  de chave→valor.

**Desabilitar preserva; remover apaga, avisando o que vai embora.** Desabilitar
tem de ser barato e reversível, ou ninguém desabilita para testar.

Um MOD que sobe de versão **traz a migração dele**, e ela é append-only pela
mesma regra das nossas. Um MOD que não sabe migrar o próprio dado não atualiza.

## Habilitar, desabilitar, e quem pode

**Pelo fio, com `Permission::AdministerServer`** — a mesma permissão de
`RenameServer` e `SetServerIcon`, semeada só no Comandante pela migração 1.

Isto contraria a primeira resposta dada nesta conversa («só quem tem a máquina na
mão») e o motivo está escrito no adendo do ADR 0032: aquele argumento é sólido e
**responde outra pergunta** — ele cobre o app que hospeda neste processo, e
*«deixa um servidor rodando por `seeled` numa VPS sem forma nenhuma de ser
nomeado por quem o administra»*. Vale igual para MOD.

## O que quem entra vê, e o que ele pode recusar

- Na **primeira vez** que entra num servidor com MODs, a pessoa vê o que vai
  baixar: nome, autor, versão, repositório, e o **`reach` declarado** — em
  particular se o MOD lê o que ela escreve.
- Aceitou uma vez, entra direto nas próximas. **Um servidor que troca de MOD
  pergunta de novo.**
- **Quem não aceita, não entra.** O MOD é parte do que aquele servidor é.

## Os bytes vêm do indexador, e isso tranca a LAN

Decidido depois de o custo ser mostrado, e por isso fica escrito como decisão e
não como descoberta:

> **Um servidor em rede local, sem internet, com um MOD habilitado, tranca todo
> mundo do lado de fora — inclusive quem hospeda.**

O cliente busca os bytes no indexador, por hash. Um MOD só existe se estiver
publicado. Isto contraria o ADR 0022, que chama o ponto de encontro de
*«opcional, trocável e não obrigatório»*, e o ADR 0029, que escreveu que *«nada
em um MOD depende de ele ter vindo de lá»*. Habilitar um MOD passa a ser assumir
que quem entra alcança o indexador.

A tela de hospedar tem de dizer isso **no momento de habilitar o primeiro MOD**,
não depois.

## Falha isolada

Um MOD que lança exceção **é desabilitado, e a sala continua.** Quem tem
`AdministerServer` vê qual MOD, quando, e a linha.

Não é polidez: é a contrapartida de escolher JavaScript. Sem isolamento de falha,
um MOD ruim é uma queda de servidor — e o defeito que o `CLAUDE.md` deste
repositório nomeia como o mais caro daqui é *«o produto sabe e não conta»*.

Vale para as duas metades: um MOD de cliente que quebra não pode levar a janela
junto.

## Discordância de versão

**O MOD não carrega, e diz.** O servidor sobe sem ele; quem tem
`AdministerServer` lê qual MOD, que API ele pede, e qual o servidor oferece. A
sala funciona, o MOD não, e ninguém instala pela metade em silêncio.

Um MOD declara **uma** versão de API. Para valer numa versão nova, o autor
publica de novo — passando pela revisão outra vez.

---

# O indexador

**É nosso**, e é a primeira coisa que este projeto hospeda além do release no
GitHub.

- **Repositório público obrigatório.** Não é um campo declarado: é requisito de
  publicação.
- **Revisão de código de cada versão.** Não só da primeira. Um MOD limpo aprovado
  cuja versão seguinte entra sozinha é uma revisão que não significa nada.
- **Catálogo assinado, busca no cliente.** Não é uma API. Com API, o indexador
  aprende cada termo digitado; com catálogo, aprende que alguém buscou o
  catálogo. O arquivo é espelhável, e a assinatura continua conferindo.
- **Sem consulta automática ao abrir.** Mesma regra do ADR 0026.
- **MOD oficial é uma assinatura conferida offline**, com chave **separada** da do
  atualizador — uma chave que atesta duas coisas deixa as duas se passarem uma
  pela outra, e a do 0026 autoriza instalar programa.
- **Remoção existe**, e uma remoção precisa alcançar quem já baixou. É a mesma
  peça da lista de revogação de versões, e as duas devem ser a mesma coisa.

**O que continua verdade depois de tudo isso: quem serve os bytes vê quem os
pediu.** Minimizado, espelhável — e verdade. É o mesmo saldo do degrau 4 do ADR
0022, e a mesma honestidade.

**A revisão não escala, e isso é um fato e não um risco.** Cada versão de cada
MOD passa por olho humano nosso. Um catálogo que cresce é um catálogo que
consome mais do nosso tempo, linearmente, para sempre. A automação ajuda no que é
mecânico — esquema válido, tamanho, assinatura, o repositório existe e bate com o
artefato — e não substitui a leitura, porque o que a leitura procura é intenção.

---

# Os seis exemplos, e o que cada um exercita

Os exemplos são do dono e são o melhor teste deste desenho: se um deles não cabe,
o desenho está errado.

| MOD | o que ele exercita |
|---|---|
| **Cor** | valores de token; a parte que o ADR 0029 já tinha resolvido |
| **Distribuição em tela** | MOD de cliente com alcance de janela inteira |
| **Perfil (banner, descrição, moldura, GIF)** | campos novos numa entidade nossa + mídia + animação livre |
| **Túnel** | rede de saída a partir do servidor; supera o degrau 5 do ADR 0022 |
| **Bot de chat** | runtime no servidor + estado + `reach` de leitura declarado |
| **Salas de RPG** | estado + chamada de verbos nossos pela API |

Dois deles carregam decisões que este documento registra explicitamente:

- **Perfil com GIF** reabre `SetPersonIcon` — hoje **PNG e só PNG, 8 KiB, lado
  ≤ 256 px**, com GIF recusado por construção porque *«animaria numa placa»* — e
  reabre «movimento é diagnóstico» de `specs/07`. As duas caem porque a regra do
  produto base não alcança MOD.
- **Túnel** é nominalmente o que o ADR 0022 recusou, citando playit.gg e ngrok:
  *«Não é "sem servidor": é o servidor de outra pessoa, escondido… seria o modelo
  do Discord com outro nome.»* Permitido por MOD, com uma tela que diz **quem** e
  **o quê**: todo o áudio e o texto daquele servidor passam por um terceiro
  nomeado.

---

# Ordem de entrega

Dentro de um desenho só, porque foi pedido — mas com a ordem escrita, porque
juntar tudo esconderia o caro atrás do fácil.

1. **Manifesto, instalação local, habilitar/desabilitar, migração 11.** Sem fio e
   sem indexador. Prova o formato antes de ele virar contrato com terceiros.
2. **`mod://`, CSP, carga do lado do cliente.** Entrega cor, layout e perfil — a
   metade dos exemplos que não precisa de servidor.
3. **Spike do interpretador**, com números, e a escolha registrada.
4. **Runtime no servidor, API `v1` congelada, quintal de dados, falha isolada.**
   Entrega bot, RPG e túnel.
5. **Verbos de protocolo, anúncio por hash, tela de aceite, «não entra sem».**
6. **Multi-versão**, incluindo launcher, manifesto de versões e revogação. Precisa
   estar de pé **antes** de o indexador abrir para a comunidade, porque é o que
   impede o catálogo de apodrecer na atualização seguinte.
7. **Indexador**: catálogo assinado, publicação, revisão, remoção.

O corte 5→6 é o momento em que este projeto para de ser sobre MODs e passa a ser
sobre distribuição. Vale ser dito em voz alta antes de chegar lá.

---

# O que fica sem saída

Cinco, e nenhuma tem resposta boa. Estão aqui porque um desenho que só louva a
opção escolhida não serve para nada daqui a um ano.

**A revisão de código é a única defesa, e ela é humana.** Não há caixa de areia,
por decisão. Um MOD malicioso que passe pela leitura roda com acesso real na
máquina de quem hospeda e na de todo mundo que entrou. A profundidade da defesa é
a atenção de quem revisa, num dia qualquer, no vigésimo MOD da semana.

**Ler o fonte não é entender o fonte.** JavaScript ofuscado passa em qualquer
esquema e falha em qualquer leitura — mas ofuscado é fácil de recusar. O que não
é fácil é código limpo que faz uma coisa a mais, discreta, na décima função.

**A LAN offline fica trancada**, e foi escolhido assim depois de o custo ser
mostrado. Um produto cujo argumento é «o servidor é seu» passa a ter um caso em
que entrar depende de um serviço nosso estar no ar.

**Guardar toda versão para sempre é um compromisso sem fim conhecido.** Disco
nosso, disco de quem instala, e cada release virando artefato permanente. A lista
de revogação é a válvula, e ela reintroduz uma consulta que o ADR 0026 tinha
orgulho de não ter.

**Um MOD pode ser feio, lento e ruim, e passar em tudo.** O produto pode tornar a
leitura possível e não pode torná-la provável — e a conferência que sobrou mede
tamanho e assinatura, não mede gosto nem intenção. A resposta que resta é a mesma
de sempre: a pessoa desabilita, e isso só funciona porque ela consegue.

---

# Custo de reverter

**Baixo antes do primeiro MOD publicado.** Um manifesto, uma migração aditiva,
um handler de protocolo, uma tela e um diretório.

**Alto depois**, e em dois lugares diferentes: a API congelada não pode ser
editada por construção, e o multi-versão promete que toda versão publicada
continua alcançável. As duas promessas são o produto, não uma implementação dele
— quebrá-las é quebrar MOD de terceiro e trancar servidor de gente, que são as
duas coisas que este desenho existe para não fazer.
