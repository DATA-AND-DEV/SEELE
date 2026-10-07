# Como se faz um MOD

> Para quem vai escrever um. A decisão de por que MODs existem e o que eles
> alcançam está no [ADR 0045](adr/0045-mods-o-produto-base-tem-regras-e-um-mod-nao.md),
> revisto pelo [ADR 0049](adr/0049-um-mod-deixa-de-rodar-na-janela-do-produto.md);
> o indexador, em `indexador-de-mods.md`, no repositório `SEELE-MODS-INDEXER`.
>
> **Este guia é da API 5**, a que a v0.15.0 oferece (`MOD_API_VERSION`, em
> `crates/seele-proto/src/mods.rs`). O contrato é [`api/v5.json`](../api/v5.json).
> O MOD de referência, `apps/seele-app/testes/mod-de-referencia/`, chama tudo o
> que a API oferece, e a bateria o executa: quando este guia e ele discordarem,
> vale ele.

**Um MOD é JavaScript.** Não há SDK, não há build, não há passo de compilação.
O que você escreve é o que roda, e é também o que passa pela avaliação — os três
são o mesmo arquivo, de propósito.

## O que um MOD é, em disco

```
meu-mod/
├── mod.json
├── cliente/main.js     roda na máquina de quem entra, num contexto só dele
├── servidor/main.js    roda na máquina de quem hospeda
└── som/toque.wav       a mídia que o MOD traz, quando traz
```

As duas metades são opcionais, mas não as duas de uma vez: um manifesto sem
nenhuma é recusado (`empty`). Um MOD de cor não tem `servidor/`; um bot de chat
não tem `cliente/`.

### `mod.json`

```json
{
  "schema": 1,
  "id": "fulano/meu-mod",
  "version": "1.0.0",
  "api": 5,
  "repo": "https://github.com/fulano/meu-mod",
  "reach": ["regiao", "tema", "pedido"],
  "client": "cliente/main.js",
  "server": "servidor/main.js",
  "arquivos": ["som/toque.wav"]
}
```

| campo | o que é |
|---|---|
| `schema` | formato deste arquivo. Hoje, `1`. |
| `id` | `autor/nome`. Minúsculas, dígitos e hífen. É ele, e não o nome da pasta, que identifica o MOD: instalado, o pacote mora numa pasta com o hash do conteúdo. |
| `version` | sua. Nunca é interpretada por nós. |
| `api` | a versão da API contra a qual você escreveu. Hoje, `5` — ver `api/v5.json`. |
| `repo` | seu repositório público. Obrigatório. |
| `reach` | o que você declara alcançar, em palavras suas; o anúncio do servidor leva até 16, de até 32 bytes cada. A tela de aceite mostra isto antes de baixar. **É declaração, e nada a confere**: o que o MOD alcança de fato é o que a versão da API dele oferece. |
| `client`, `server` | caminhos das metades, relativos à pasta. |
| `arquivos` | a mídia que o MOD traz para mostrar ou tocar, relativa à pasta. Até 16, e só o que está nesta lista é servido. O tipo vem dos bytes, nunca da extensão. |
| `state` | opcional: a versão do esquema dos seus dados, para as suas migrações. Lida, e ainda não usada pelo produto. |

**Chave desconhecida é recusada, e é de propósito.** `vesion` com um `s` a menos
instalaria calado um MOD sem versão, e você nunca ficaria sabendo. A recusa é a
única forma de retorno que este formato te dá.

**Este build executa as APIs 3, 4 e 5, e só elas** (`APIS_ACEITAS`). Um `api`
mais novo é recusado como `api-too-new`; um `api` 1 ou 2 é recusado como
`api-too-old`, porque aquelas rodavam na janela do produto, e isso não volta. O
produto não roda pela metade. Declarar 3 ou 4 continua valendo, e dá ao MOD só o
que aquela versão tinha — ver «O que existe lá dentro», abaixo.

## A metade de cliente

Roda na máquina de quem entra, **num contexto QuickJS só dela**, e não na janela
do produto. Lá dentro não existem `document`, `window`, `fetch`, `localStorage`,
`indexedDB` nem o global do Tauri — e não existem porque ninguém os ligou, e não
porque alguém os apagou no começo do arquivo: o segundo caso se contorna. Foi a
ruptura da API 3, e o ADR 0049 diz por quê: um MOD que roda na página não tem
como sumir quando você sai do servidor, e sumir é uma promessa do produto.
Destruir o contexto destrói o MOD — temporizador, ouvinte e promessa atrasada
morrem junto, sem ele precisar cooperar.

**Você declara, e quem desenha é o produto**, com `createElement` e
`textContent`. Não há forma que aceite HTML: o conteúdo vem de código de
terceiro, e interpretá-lo como marcação devolveria a página ao MOD por outro
caminho.

```js
// cliente/main.js
const EU = "fulano/meu-mod";

async function desenhar(vezes) {
  await SeeleUI.regiao([
    { forma: "titulo", chave: "t", dentro: "PLACAR" },
    { forma: "texto", chave: "v", dentro: `vezes: ${vezes}` },
    { forma: "botao", chave: "mais", dentro: "MAIS UM" },
  ]);
}

SeeleUI.aoEvento(async (evento) => {
  if (evento.nome === "botao" && evento.chave === "mais") {
    // Canal 0: esta pergunta não é sobre canal nenhum.
    const resposta = await SeeleMods.request(EU, 0, { op: "contar" });
    await desenhar(resposta.vezes);
  }
});

async function comecar() {
  // Um tema recusado rejeita com o motivo, e o MOD segue sem ele.
  await SeeleUI.tema({ acento: "#00b7ff" }).catch(() => {});
  await desenhar(0);
}

comecar();
```

### O que existe lá dentro

| | o que faz |
|---|---|
| `SeeleMods.request(id, canal, valor)` | pergunta à sua metade de servidor, que responde pelo `aoPedir`. `canal` é um identificador de canal, e `0` quer dizer «nenhum». A promessa resolve com a resposta já lida do JSON. |
| `SeeleMods.snapshot()` | o estado que a janela já tem: você, quem está, as salas e os canais. |
| `SeeleUI.regiao(conteudo)` | desenha a região do MOD. Chamar de novo substitui o conteúdo inteiro, e o produto reconcilia por `chave`: o que não mudou não é tocado, e quem está digitando não perde o foco. As trinta formas estão em `formas`, no `api/v5.json`. |
| `SeeleUI.tema(valores)` | pede seis cores, densidade, fonte, arredondamento e brilho — os nomes de `tema`, no `api/v5.json` —, só nesta sessão. O produto recusa o que não alcança contraste de 4,5:1, e tira o tema inteiro quando você sai. |
| `SeeleUI.cartoes(cartoes)` | um cartão por pessoa, na lista de pessoas do produto, numa gramática menor: nada que receba foco ou clique. |
| `SeeleUI.aoEvento(fn)` | o que a pessoa faz no que o MOD desenhou volta por aqui, sem número e sem resposta. Um ouvinte só, e o último vence. |
| `SeeleUI.pedaco(arquivo, inicio)` e `SeeleUI.soltar(arquivo)` | leem em pedaços o arquivo que alguém escolheu pela forma `arquivo`, e o devolvem. O MOD recebe identificador, tipo e tamanho, e nunca o caminho. |
| `SeeleUI.capacidades()` | o que a versão da API que você declarou oferece. |
| `SeeleUI.superficies` e `SeeleUI.contribuicoes` | da API 4 em diante: página, painel, diálogo e aviso montados pelo produto, e os pontos da interface do SEELE onde um MOD pode entrar (`superficies` e `contribuicoes`, no `api/v5.json`). |
| `SeeleUI.enviar(arquivo, token)` | da API 5: manda à sua metade de servidor uma imagem escolhida, até 10 MiB, por um fluxo próprio. A metade de servidor autoriza antes, com `volume.esperar`. |
| `setTimeout`, `setInterval` e os dois `clear` | a fachada do relógio do anfitrião, com até 256 de pé; acima disso devolvem `0`, que é o «não deu». |

**O que a versão não tem não existe**, e não falha em tempo de execução. Um
pacote que declara `api: 3` não tem `SeeleUI.superficies` nem
`SeeleUI.contribuicoes`: elas são `undefined`, e chamá-las é um `TypeError` na
primeira linha. Pergunte a `SeeleUI.capacidades()` antes, e o mesmo arquivo roda
nas três versões.

**O que não existe, e onde mora:**

- **rede**: não há `fetch`. Rede é da metade de servidor, `mundo.buscar`;
- **o que precisa durar**: não há `localStorage` nem `indexedDB`. Vai para o
  servidor, pelo `aoPedir` e pelo `dados`, que é onde as permissões valem;
- **`console`**: só na versão seguinte à 0.15.0. Na 0.15.0 ele não existe — ver
  «Quando o MOD não faz o que devia».

**Cada chamada atravessa como uma mensagem de até 12 KiB**, contados em bytes de
UTF-8. Uma maior rejeita com `mensagem-grande`, dizendo o tamanho e o teto. E
até oito pedidos esperam resposta ao mesmo tempo: o nono rejeita com
`too-many-requests`.

## A metade de servidor

Roda num QuickJS da máquina de quem hospeda, um por MOD. Você define duas
funções, e o produto põe quatro objetos globais.

```js
// servidor/main.js
globalThis.aoPedir = (contexto, pedido) => {
  const o_que = JSON.parse(pedido);
  if (o_que.op !== "contar") {
    return JSON.stringify({ erro: `operação desconhecida: ${o_que.op}` });
  }
  const vezes = Number(dados.vezes ?? "0") + 1;
  dados.vezes = String(vezes);
  return JSON.stringify({ vezes });
};

globalThis.aoAcontecer = (momento, carga) => {
  const evento = JSON.parse(carga);

  if (momento === "MessageReceived" && evento.texto.startsWith("$dado")) {
    const rolagem = 1 + Math.floor(Math.random() * 20);
    dados.ultimaRolagem = String(rolagem);
    mundo.registrar(`rolou ${rolagem}`);
  }
};
```

### `aoPedir(contexto, pedido)`

É quem responde ao `SeeleMods.request` da janela. Os dois argumentos chegam como
JSON, e a resposta volta como JSON em texto — `JSON.stringify`.

- **`contexto` é o que o servidor sabe**: `person`, quem pediu; `channel`, o
  canal, ou `null` quando a janela mandou `0`; `admin` e `write`, se essa pessoa
  administra o servidor e se pode escrever nos canais. **A autorização sai
  daqui, e nunca do pedido**: um MOD que confiasse num campo do pedido deixaria
  quem escreve a chamada escolher a própria permissão.
- **`pedido` é o que a janela mandou**, o terceiro argumento do `request`.
- **Uma resposta acima de 1 MiB é recusada**, e o `dados` só é gravado quando a
  chamada termina bem — como no `aoAcontecer`.

### `aoAcontecer(momento, carga)`

`momento` é o nome do evento — **o mesmo nome que o protocolo usa**, para que um
relatório de defeito que diga `MessageReceived` ache a mesma palavra no
protocolo, no `api/v1.json` e no seu código.

**Só cinco momentos são entregues**: `PersonJoined`, `PersonLeft`,
`MessageReceived`, `MessageEdited` e `MessageRemoved`. O `api/v1.json` lista 21,
e as versões seguintes os herdam; os outros dezesseis nunca foram entregues, e
um MOD que espera por `ChannelCreated` espera para sempre, sem erro
([`api/README.md`](../api/README.md), «Os quatro blocos»).

`carga` é JSON. Sempre. Faça `JSON.parse`.

Um MOD sem `aoAcontecer` não é erro: é um MOD cuja metade de servidor existe
para outra coisa.

### `dados` — o seu quintal

Um objeto de texto para texto. O que estiver nele ao fim da chamada é gravado; o
que estava lá chega preenchido.

```js
dados.placar = String(Number(dados.placar ?? "0") + 1);
```

Três coisas que valem saber:

- **Uma chamada é transacional.** Se o seu código lançar no meio, **nada** é
  gravado. Meia escrita é pior que nenhuma, porque você não teria como saber
  qual metade entrou.
- **Só texto.** Números viram texto; objetos, `JSON.stringify`.
- **Teto de 256 KiB**, e ele **recusa** em vez de aparar. Um quintal que
  descartasse a entrada mais velha em silêncio seria um quintal que você não
  pode acreditar.

### `arquivos` — a sua pasta

```js
arquivos.escrever("fichas/coelho.json", JSON.stringify(ficha));  // → true/false
arquivos.ler("fichas/coelho.json");                              // → texto ou null
arquivos.listar();                                               // → ["fichas/coelho.json"]
arquivos.apagar("fichas/coelho.json");                           // → true/false
```

Tudo dentro da pasta de dados do MOD **naquele servidor** —
`servidores/<servidor>/mod-data/<autor>/<nome>/`, na pasta de configuração de
quem hospeda —, e **nada fora**. Uma por servidor hospedado, e não uma por
máquina: o mesmo MOD em dois servidores não lê o que o outro escreveu. `..` é
recusado, caminho absoluto é recusado, e a recusa é a mesma resposta para todos
os casos.

**Por que só a sua pasta**, num sistema em que o resto é liberdade total: ao lado
dela ficam o `identity.key`, os `pins` e o banco com todas as conversas. Disco
inteiro entregaria a chave privada de identidade de quem te hospeda, e aí o que
cai não é uma regra de produto — é a garantia de que aquela pessoa é ela mesma.

Teto de 4 MiB por arquivo.

### `mundo` — rede, relógio, registro

```js
const resposta = mundo.buscar("https://api.exemplo.com/x");  // texto ou null
const agora = mundo.agora();                                  // segundos desde 1970
mundo.registrar("o que aconteceu");                           // vai para o log de quem hospeda
```

- **`http` e `https` e mais nada.** `file://` é recusado, senão a rede faria o
  que a pasta impede no disco.
- **Teto de 2 segundos por busca**, e ele existe por um motivo que vale você
  saber: o servidor roda um momento por vez, então uma busca lenta sua atrasa
  **todos os outros MODs** e a fila de eventos atrás deles. Dois segundos é o
  pior caso que alguém aceitou pagar por você.
- **Resposta acima de 4 MiB é descartada.**
- `mundo.registrar` sempre carrega o seu `id`. Você não escolhe o prefixo — uma
  linha cuja origem você pudesse forjar seria uma linha que ninguém pode usar
  para decidir qual MOD desligar.

### `volume` — imagens grandes, da API 5

Para a imagem que não cabe numa mensagem: até 10 MiB, sem passar pelo QuickJS.
Os bytes vão por um fluxo próprio, direto ao disco
([ADR 0048](adr/0048-mods-ganham-o-caminho-de-volume-que-os-anexos-ja-tem.md)).

```js
// dentro do aoPedir: autoriza um envio de quem pediu, por 30 segundos
volume.esperar(token, "retratos/coelho.png", ["png", "jpeg"], 30);  // → true/false
volume.tamanho("retratos/coelho.png");                              // → bytes ou null
volume.apagar("retratos/coelho.png");                               // → true/false
```

- **`volume.esperar` só vale dentro do `aoPedir`.** A pessoa que pode enviar é a
  que está sendo atendida, e quem diz isso é o servidor, e não um argumento.
- **Os tipos são `png`, `jpeg`, `webp` e `gif`**, e o que chega é conferido
  pelos bytes, e não pelo nome.
- Do outro lado, a janela manda com `SeeleUI.enviar(arquivo, token)`, e a
  promessa resolve depois da gravação. Ler de volta é o bloco `volume` do
  `api/v5.json`, com `volume.servir`.

## Os dois tetos que te cortam no servidor

| | quanto | o que acontece |
|---|---|---|
| memória | 8 MiB por MOD | a chamada morre; **o seu contexto sobrevive** |
| tempo | ~500 consultas ≈ 225 ms | a chamada morre; o seu contexto sobrevive |

Um laço infinito não derruba a sala: ele derruba a sua chamada. Mas um
`aoAcontecer` que falha **desliga** o MOD, e o desligamento é gravado — então um
bug que lança em todo evento tira o seu MOD do ar até alguém religá-lo.

Os 225 ms não são de relógio: são de trabalho. Uma máquina lenta te dá o mesmo
tanto de operações e demora mais, em vez de te cortar antes.

## Testar antes de publicar

Em CONFIGURAÇÕES › MODS › INSTALADOS, **INSTALAR MOD DE UMA PASTA** aponta para
a pasta do seu MOD. O `mod.json` é lido antes de qualquer byte ser copiado, e o
que não serve é recusado pelo nome — `api-too-old`, `malformed-id`, `empty` e o
resto. Instalar não liga: depois, hospede e habilite.

O pacote instalado mora numa pasta com o hash do conteúdo
(`mod-packages/<hash>/`, na pasta de configuração), e por isso **editar os
arquivos lá dentro não funciona**: um pacote mexido depois de instalado é
recusado, e nomeado. Mudou o MOD, instale de novo.

**Erros do seu MOD aparecem no `seele.log`**
(`~/.config/seele/seele.log`), com o seu `id` num campo próprio, `mod_id=`: os
da metade de servidor no de quem hospeda, e os da metade de janela no de cada
máquina que roda o MOD. A seção seguinte diz como lê-los.

> **O que falta, e é honesto dizer:** não há `seele mod new`, não há definições
> de tipo para o seu editor, e não há como rodar um MOD sem hospedar um servidor.
> O ADR 0045 promete as definições de tipo e elas ainda não existem. Enquanto
> não existirem, o retorno que você tem é o log e a aba DIAGNÓSTICO.

## Quando o MOD não faz o que devia

Esta seção vale a partir da versão seguinte à 0.15.0, a que trouxe o `console`
da metade de janela. O que mudou nela para quem escreve MOD — o `console`, o som
e os tetos — está em [`api/README.md`](../api/README.md), «O que muda para quem
escreve MOD nesta versão».

### O `console`, e onde ele escreve

A metade de janela tem `console`, e cada chamada vira **uma linha no
`seele.log` da máquina que roda o MOD**, no nível que você escolheu. A metade
de servidor não tem: lá, `console.log(…)` lança `ReferenceError`, e um
`aoAcontecer` que lança desliga o MOD. O registro dela é o `mundo.registrar`,
que vai ao log de quem hospeda com o mesmo `mod_id=`.

| você escreve | a linha sai em |
|---|---|
| `console.debug`, `console.trace` | DEBUG — que o `seele.log` **não grava** por padrão |
| `console.log`, `console.info`, `console.dir`, `console.table` | INFO |
| `console.warn` | WARN |
| `console.error`, e o `console.assert` cuja condição é falsa | ERROR |

`group`, `time`, `count` e `clear` existem e não fazem nada. O erro que o seu
código não pegou também chega, em ERROR. A exceção no topo e a promessa
rejeitada que ninguém pegou até o fim da volta levam o texto e a primeira linha
da pilha, que diz onde; a de um temporizador e a de um ouvinte levam o texto e
a pilha inteira, até o teto da linha. A pilha só vem quando o lançado é um
`Error`.

**Tem teto.** Trinta e duas linhas de uma vez, e depois quatro por segundo;
cada linha é cortada em 512 caracteres. O que o teto segurou não some calado:
a linha seguinte diz quantas foram seguradas (`suprimidas=`).

**Na 0.15.0 o `console` não existe**, e `console.warn(…)` lá é um
`ReferenceError`. Um MOD que precisa rodar nas duas versões pergunta antes:
`if (typeof console !== "undefined") console.warn("…")`.

### Ler a linha

Cada linha do `console` sai assim:

```text
… WARN seele_app: console do MOD mod_id=fulano/meu-mod texto="o retrato não veio"
```

O texto vem entre aspas e escapado: uma quebra de linha nele não vira outra
linha do registro. Para ver só o seu MOD — o `console`, as mídias que a janela
recusou e o que o servidor disse dele, que usam o mesmo campo:

```sh
grep "mod_id=fulano/meu-mod" ~/.config/seele/seele.log
```

O arquivo fica em `~/.config/seele/` no macOS e no Linux (ou em `$SEELE_HOME`,
quando definido) e em `%APPDATA%\tech.datadev.seele\` no Windows. Para gravar
também o `console.debug`, abra o app com `RUST_LOG=info,seele_app=debug` no
ambiente.

### A aba DIAGNÓSTICO

Em CONFIGURAÇÕES › MODS › DIAGNÓSTICO, QUEM PINTA CADA LUGAR: os onze pontos
onde um MOD pode entrar nesta versão, sempre os onze, e não só os que alguém
disputa. Para cada um, quem desenha agora (o SEELE, um MOD, ou um para cada
pessoa que declarou), quem pediu e perdeu, a escolha desta máquina, e a mídia
em uso — quantas carregando, prontas e recusadas, com o motivo da recusa. A
mídia da região do MOD e a das páginas dele ainda não entram nessa conta
(`docs/pendencias.md`, pendência 49, item 5): uma recusa ali está no
`seele.log` e no evento que o seu MOD recebe. «Nenhum MOD usa este lugar agora»
é a resposta que você mais precisa ler quando o seu não aparece. É só leitura:
escolher quem desenha continua em INSTALADOS, «QUEM DESENHA O QUE».

### O modo de desenvolvedor

Na mesma aba, MODO DE DESENVOLVEDOR contorna na tela cada lugar onde um MOD pode
entrar, com o nome do ponto como a API o chama — é o que você escreve em
`ponto`. Os contornos não recebem clique nem teclado: a conversa continua
funcionando por baixo, e a barra de espaço continua falando. Vale só na
máquina em que você ligou.

## Publicar

1. **Clone o repositório base**, faça o seu, e dê `push` no **seu** repositório
   público.
2. **Peça inclusão pelo site**, com a URL do repositório.
3. **A avaliação roda**: estrutura, e o código inteiro contra código malicioso,
   comunicação com terceiros e tentativa de invasão.
4. **O veredito**, e são três:

| | o que significa |
|---|---|
| **verificado** | passou. Entra no catálogo com assinatura de verificado. |
| **publicado com notas** | não passou limpo, mas o que ele faz é legítimo — uma integração com um terceiro conhecido, por exemplo. Entra **sem** verificação, e as notas de segurança aparecem na tela de quem instala. |
| **negado** | o que ele faz lesa quem instala. Não entra. |

**O commit avaliado é fixado no catálogo.** Um `push` depois da aprovação não
muda o que as pessoas baixam — ele exige uma solicitação nova. É o que faz a
avaliação valer alguma coisa.

## As regras que não mudam

- **Uma versão publicada nunca é editada.** Versão nova é caminho novo. É a
  mesma disciplina das migrações do servidor, e vale pelo mesmo motivo.
- **O `api` que você declara é uma versão só.** Um MOD de API 3 continua rodando
  num build que oferece a 5, com o que a 3 tinha; para ter o que a mais nova
  acrescenta, publique de novo — e passe pela avaliação de novo.
- **Uma versão publicada da API nunca é editada.** `api/congeladas.sha256` e o
  teste `a_api_publicada_nao_se_edita` reprovam o build quando os bytes de uma
  `vN.json` mudam. Isso não é o mesmo que a API nunca perder nada: a API 3 tirou
  a janela inteira, e por isso um MOD de API 1 ou 2 não roda mais
  ([ADR 0049](adr/0049-um-mod-deixa-de-rodar-na-janela-do-produto.md)). Renomear
  por dentro continua sendo problema nosso, e há um build vermelho que cobra
  isso de nós, não de você: o `cargo xtask check-api`.
