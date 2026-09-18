# 0048 — MODs ganham o caminho de volume que os anexos já têm

Status: **aceito** — as duas decisões abertas foram respondidas por quem
responde pelo projeto em 2026-09-18, e estão registradas abaixo com o que cada
uma custa.
Data: 2026-09-18
Sobre o commit `e90bcc16751cad0861c73c3f57bfaa78c5b3e434`.

> **As duas decisões foram tomadas**, e o desenho abaixo decorre delas:
> **sem teto de disco para MODs**, e **o servidor confere os bytes**. A seção
> «As duas decisões» guarda as alternativas que foram recusadas e o argumento
> de cada uma — não como hesitação, mas porque um dia alguém vai perguntar por
> que não há teto, e a resposta tem de estar escrita.

## O que doeu, medido

Relato de campo: «hoje eu sinto muita dificuldade em enviar imagens mais
pesadas… demora muito, até pra uma imagem de 1 MB».

A conta, com os números do produto e não de estimativa:

| | fragmentos | tempo |
|---|---|---|
| 1 MB pelo MOD PERFIS hoje | 223 | **28 s** |
| 10 MiB | 2 331 | 4,9 min |

**A hipótese de quem relatou era «o teto de bytes é baixo». Não é.** O teto de
imagem do MOD é 10 MiB e não aperta. O que aperta são três coisas empilhadas,
todas do caminho e nenhuma do tamanho:

1. **`ModRequest` carrega no máximo 12 KiB** (`control.rs`,
   `check("mod_payload", …, 12 * 1024)`). Uma imagem de 1 MB vira 1 333 336
   caracteres de base64 — cento e nove quadros de controle no mínimo absoluto.
2. **O canal de controle não é para volume, e diz isso de si mesmo.**
   `control.rs`: *«Control frames are small by design — history and other bulk
   transfers use their own streams»*. O balde é de 20 quadros por segundo
   (`taxa.rs`, `QUADROS_POR_SEGUNDO`), com rajada de 60 e `PACIENCIA = 200`
   quadros excedentes antes de a **conexão ser derrubada**.
3. **`arquivos::escrever` recebe uma `String`** e tem teto de 4 MiB por arquivo
   (`mods/arquivos.rs`). Uma imagem de 10 MiB não cabe num arquivo de MOD, por
   construção — não é limite de política, é a assinatura da função.

O MOD PERFIS contorna as três: parte a imagem em fragmentos de 6 000
caracteres, grava um arquivo por fragmento mais um índice, e enfileira os
pedidos a oito por segundo para não ser derrubado pelo balde. São 2 331
arquivos por imagem de 10 MiB. **O contorno é competente e está certo dentro do
que existe** — e é a prova de que o que falta é um caminho, não um ajuste.

## O que já existe, e é o molde

**Não há nada a inventar aqui.** O ADR 0027 resolveu exatamente este problema
para anexos, e `seele-server/src/transfer.rs` é a implementação:

- **um fluxo unidirecional por transferência.** O arquivo nunca cruza o fluxo
  de controle, nos dois sentidos. O comentário do módulo diz por quê: *«an
  ordered stream blocks itself: twenty megabytes written into it stop every
  presence event, every channel of text and every `Pong` from everybody behind
  them»*;
- **cabeçalho e depois os bytes.** Quem envia abre o fluxo e escreve o
  cabeçalho; o servidor que devolve abre um fluxo próprio;
- **a resposta volta pelo controle**, como motivo enumerado, porque é lá que o
  `specs/02-protocolo.md` guarda todo motivo;
- **nada segura o arquivo**: blocos de `BLOCK_LEN` (64 KiB) entre a rede e o
  disco, nos dois sentidos, porque o servidor é dimensionado em 1 vCPU e 512 MB;
- **prioridade**: controle acima de toda transferência, por
  `quinn::SendStream::set_priority`;
- **balde próprio de bytes**: 256 MiB de rajada, 256 KiB/s sustentados. O
  `Balde` de `taxa.rs` já é genérico (`novo(capacidade, por_segundo)`).

O que este ADR propõe é **estender esse caminho aos MODs**, e não construir um
segundo.

## O desenho

### A forma, em uma frase

O MOD autoriza pelo controle, os bytes vão por um fluxo próprio direto ao
disco, e o MOD recebe pelo controle o que chegou — tamanho e hash — para
decidir se fica.

### Passo a passo

1. **Autorização, no controle.** A janela pede ao MOD `{op:"upload-start", …}`,
   como hoje. O MOD responde com um token e **os tipos que aceita**. Nada flui
   antes disso. Esta metade já existe e está certa: o caminho do arquivo é
   gerado pelo servidor, nunca vem do pedido, e a avaliação de PERFIS 1.0.0 já
   registrou isso como a propriedade que fecha a travessia por construção.
2. **Os bytes, por fluxo próprio.** A janela abre um `uni` com um cabeçalho
   `{mod, token}` e escreve os bytes crus — **não base64**. O servidor confere
   o token, escreve em blocos de 64 KiB direto no arquivo destino dentro da
   pasta daquele MOD, e vai somando o hash.
3. **O veredito, no controle.** Terminado o fluxo, o MOD recebe
   `{op:"upload-done", token, bytes, hash, tipo}` e decide: gravar no perfil, ou
   descartar. Se ele descartar, o servidor apaga.

**O QuickJS nunca vê os bytes.** Isso não é otimização: é o que torna 10 MiB
possível. O desenho atual de PERFIS já persegue essa propriedade — o comentário
do autor diz *«Neither QuickJS nor arquivos.ler/escrever ever receives the
entire 10 MiB image»* — e a paga com 2 331 arquivos. Aqui ela sai de graça.

### A peça que faz a autorização valer: o servidor precisa saber do token

O token nasce dentro do MOD, no QuickJS, e o servidor não interpreta o estado
do MOD — nem deve. Se o cabeçalho do fluxo trouxesse só `{mod, token}`, o
servidor não teria contra o que conferir, e «autorizar» seria uma formalidade
que o cliente poderia pular: bastaria abrir um fluxo com qualquer token.

Então o MOD ganha um verbo que **registra a espera** no servidor, chamado de
dentro do `aoPedir`, no mesmo ato que responde à janela:

```js
volume.esperar({ token, caminho, tipos: ['png','jpeg','webp','gif'], prazo: 600 })
```

O servidor guarda a espera — quem pediu, qual pessoa, qual caminho, quais
tipos, até quando — e é **contra ela** que o cabeçalho do fluxo é conferido. O
caminho continua sendo escolhido pelo MOD e passando pelo `inner_path` de
sempre; o que o servidor acrescenta é que nenhum byte entra sem uma espera
registrada, com prazo, e para a pessoa certa.

Uma espera é de **uma pessoa** e vale **uma vez**: consumida no primeiro fluxo
que a case. Sem isso, um token vazado viraria um lugar de escrita permanente na
pasta do MOD, para quem o tivesse.

### O que muda em `mods/arquivos.rs`

Um arquivo de volume não é um arquivo de MOD comum, e misturá-los apagaria o
teto de 4 MiB que protege o disco de quem hospeda hoje. Proposta: eles vivem
num espaço nomeado à parte — `volume/` dentro da pasta do MOD —, com as regras
próprias abaixo. `ler`/`escrever`/`apagar` continuam como estão, para tudo o
mais, com o mesmo `inner_path` e o mesmo teto.

O MOD ganha três verbos que **não carregam bytes**: `volume.tamanho(nome)`,
`volume.apagar(nome)` e `volume.servir(nome)` — o último pede ao servidor que
abra um fluxo de volta para a janela. O MOD nomeia o arquivo; nunca recebe nem
devolve conteúdo.

### O que isto faz com o tempo

Com balde de bytes de anexo (256 KiB/s sustentados, 256 MiB de rajada), 10 MiB
passam **dentro da rajada**, na velocidade do enlace. O caso de 1 MB deixa de
ter fragmento e deixa de ter fila: é um fluxo, e leva o que a rede levar.

## As duas decisões que são de quem responde pelo projeto

### 1. Quanto disco um MOD pode ocupar

Hoje existe teto **por arquivo** (4 MiB) e nenhum teto **total**. Isso é
seguro enquanto escrever exige passar pelo QuickJS a 20 quadros por segundo: o
ritmo é o limite de fato. **Um caminho de volume remove esse limite de fato**, e
sem um teto total um MOD enche o disco de quem hospeda.

Os anexos resolveram com teto mais «o mais velho sai» (ADR 0027). Para MODs, o
«mais velho sai» é pior do que parece: o arquivo mais velho pode ser o avatar
de alguém que não entra há um mês, e apagá-lo é apagar o perfil dela sem avisar.

Três respostas possíveis, e nenhuma é obviamente certa:

- **teto total por MOD, e a escrita falha quando estoura.** Previsível, e a
  falha é do MOD, que sabe explicá-la a quem está usando. Quem hospeda precisa
  limpar à mão quando encher;
- **teto por MOD com «o mais velho sai»**, como anexos. Nunca enche, e apaga
  coisa de alguém sem que ninguém peça;
- **teto por pessoa dentro do MOD.** Justo, e é o único que impede uma pessoa
  de gastar a cota de todas — mas exige que o servidor saiba de quem é cada
  arquivo, o que hoje ele não sabe: a pasta é do MOD, e o MOD é quem nomeia.

Minha recomendação foi a primeira, com o teto dito ao MOD na autorização.

**A decisão foi outra: nenhum teto.** Ela é de quem responde pelo projeto, foi
tomada com a consequência à vista, e o desenho a segue. O que ela significa, dito
por extenso para não virar surpresa de outra pessoa:

- o teto de 4 MiB por arquivo continua valendo para `arquivos::escrever`, que é
  o caminho antigo. **O caminho de volume não tem teto**, nem por arquivo nem
  por MOD;
- o limite de fato que existia — o ritmo do controle, 20 quadros por segundo —
  **deixa de existir** neste caminho. Quanto disco de quem hospeda um MOD ocupa
  passa a depender inteiramente de quem usa o MOD;
- não há «o mais velho sai». Nada é apagado por conta própria, que era o custo
  que se queria evitar, e ele foi evitado.

**E é por isso que uma coisa deixa de ser opcional.** Sem teto, quem hospeda
precisa **ver**: quanto cada MOD ocupa, e desde quando. Um produto que deixasse
o disco encher sem ter onde olhar seria «o produto sabe e não conta», que é a
falha que este repositório mais paga. A visibilidade entra junto com o caminho,
e não depois — ela é a metade que torna «sem teto» uma decisão e não um
descuido.

### 2. Quem confere os bytes

Hoje o MOD confere: cabeçalho `data:image/(png|jpeg|webp|gif)` e bytes mágicos
no fragmento 0, alfabeto de base64 em cada fragmento, e contagem de bytes
decodificados no último. É uma conferência boa, e ela **deixa de ser possível**
quando os bytes não passam pelo QuickJS.

- **o servidor confere**, com a mesma tabela de quatro formatos que a prévia de
  anexo já usa — reúso, e não uma segunda cópia da regra. O MOD declara na
  autorização quais tipos aceita;
- **ninguém confere**, e o MOD recebe o hash e o tipo declarado. Mais simples,
  e move para os MODs uma responsabilidade que eles não têm como cumprir sem
  ver os bytes.

**Decidido: a primeira.** E ela tem um efeito colateral bom: a conferência
passa a ser do produto, igual para todo MOD, em vez de cada autor reescrever a
sua — e o ADR 0045 já diz que o produto base tem regras.

## Consequências

**Ganha-se** o caminho que falta: MODs deixam de empurrar volume por um canal
que existe para «uma mensagem de texto e um `Ping` a cada cinco segundos».
PERFIS perde 2 331 arquivos por imagem, a fila de 125 ms, o índice e a coleta
de lixo orçamentada — todos existem para contornar a ausência disto.

**Perde-se** simplicidade no protocolo: mais um tipo de fluxo, mais um
cabeçalho, mais um balde. E perde-se o limite de disco que existia sem ninguém
ter decidido — o ritmo do controle. Ele era acidental e agora não existe; no
lugar dele fica a visibilidade, que é decisão e não acidente.

**O que este ADR não resolve:** dois MODs enviando ao mesmo tempo de conexões
diferentes não se ordenam entre si, como o 0027 já registra para anexos, e voz
continua competindo pelo mesmo gargalo de subida por ser datagrama. Não há boa
resposta para isso num enlace doméstico, e este documento não vai fingir que há.
