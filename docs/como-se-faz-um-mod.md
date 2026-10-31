# Como se faz um MOD

> Para quem vai escrever um. A decisão de por que MODs existem e o que eles
> alcançam está no [ADR 0045](adr/0045-mods-o-produto-base-tem-regras-e-um-mod-nao.md),
> revisto pelo [ADR 0049](adr/0049-um-mod-deixa-de-rodar-na-janela-do-produto.md);
> o indexador, em `indexador-de-mods.md`, no repositório `SEELE-MODS-INDEXER`.
>
> **Este guia é da API 8** (`MOD_API_VERSION` e `APIS_ACEITAS`, em
> `crates/seele-proto/src/mods.rs`), que alarga a 7 com a declaração de onde o
> MOD roda (`plataformas`, abaixo); a 7 e a 6 continuam rodando nesta linha. O
> contrato é [`api/v8.json`](../api/v8.json); o que a 8 acrescenta está em
> [`api/README.md`](../api/README.md), «A API 8». **A 0.18 não executa a 8.** O
> MOD de referência, `apps/seele-app/testes/mod-de-referencia/`, chama tudo o
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
nenhuma é recusado (`empty`). Um MOD que só mostra coisas não precisa de
`servidor/`; um bot de chat não precisa de `cliente/`.

### `mod.json`

```json
{
  "schema": 1,
  "id": "fulano/meu-mod",
  "version": "1.0.0",
  "api": 8,
  "repo": "https://github.com/fulano/meu-mod",
  "reach": ["superficie", "contribuicao", "pedido"],
  "client": "cliente/main.js",
  "server": "servidor/main.js",
  "arquivos": ["som/toque.wav"],
  "tema": true,
  "plataformas": ["app", "navegador"]
}
```

Um MOD que hospeda um jogo declara mais três chaves (a metade de máquina, adiante,
explica o porquê de cada uma):

```json
{
  "schema": 1,
  "id": "fulano/jogo",
  "version": "1.0.0",
  "api": 8,
  "repo": "https://github.com/fulano/jogo",
  "reach": ["tunel"],
  "client": "cliente/main.js",
  "server": "servidor/main.js",
  "maquina": "maquina/main.js",
  "plataformas": ["app"],
  "poderes": ["porta"],
  "permissoes": [{ "acao": "entrar-no-jogo", "nome": "Entrar no jogo", "todos": true }]
}
```

| campo | o que é |
|---|---|
| `schema` | formato deste arquivo. Hoje, `1`. |
| `id` | `autor/nome`. Minúsculas, dígitos e hífen. É ele, e não o nome da pasta, que identifica o MOD: instalado, o pacote mora numa pasta com o hash do conteúdo. |
| `version` | sua. Nunca é interpretada por nós. |
| `api` | a versão da API contra a qual você escreveu. Hoje, `8`; a `7` e a `6` continuam aceitas. |
| `repo` | seu repositório público. Obrigatório. |
| `reach` | o que você declara alcançar, em palavras suas; o anúncio do servidor leva até 16, de até 48 bytes UTF-8 cada na 0.17 (32 na 0.16), e a 0.17 recusa na instalação o que o anúncio recusaria (`alcances-demais`, `alcance-grande-demais`). A tela de aceite mostra isto antes de baixar. **É declaração, e nada a confere**: o que o MOD alcança de fato é o que a API 8 oferece. |
| `client`, `server` | caminhos das metades, relativos à pasta. |
| `arquivos` | a mídia que o MOD traz para mostrar ou tocar, relativa à pasta. Até 16, e só o que está nesta lista é servido. O tipo vem dos bytes, nunca da extensão. |
| `tema` | opcional: `true` abre a porta do tema do servidor, `mundo.tema`, na metade de servidor (ver «O tema»). Sem `server`, o manifesto é recusado (`tema-sem-metade-de-servidor`): sem quem a abra, a porta nunca pintaria nada. |
| `plataformas` | opcional: onde o MOD roda. Os valores são `"app"` e `"navegador"`. **Ausente quer dizer as duas.** **Declarar exige `"api": 8`**: a chave num manifesto da 7 ou da 6 é recusada (`PlataformasPedemApiNova`, no shell `plataformas-pedem-api-nova`), e não ignorada, mesmo escrita como `[]`, porque o que se confere é a presença da chave. Escrita e vazia é recusada (`PlataformasVazias`, `plataformas-vazias`); repetir um valor também (`PlataformaRepetida { posicao }`, `plataforma-repetida`); um valor desconhecido faz o manifesto inteiro ser recusado como malformado. **O celular é o navegador**: um MOD só `"app"` não carrega nele. Uma casca que não está na lista pula o MOD e diz qual plataforma ele pede («só funciona no app», «só funciona no navegador»). |
| `poderes` | opcional: o que o MOD pode fazer além de pintar e responder. Hoje, `"porta"`: o túnel de quem joga até o servidor de jogo de outra pessoa (`SeeleUI.tunel`, ver «A metade de máquina»). `"programa"` está **reservado à parte seguinte** e não faz nada nesta: é um nome que o manifesto já lê, e não um poder que o produto exerça. **Declarar exige `"api": 8`** (`PoderesPedemApiNova`, `poderes-pedem-api-nova`), e a chave escrita e vazia é recusada (`PoderesVazios`, `poderes-vazios`), como repetir um valor (`PoderRepetido { posicao }`, `poder-repetido`). **Exige `"plataformas": ["app"]`**: o túnel abre um socket na máquina de quem joga, o navegador não tem como, e um MOD que também rodasse lá (`plataformas` ausente quer dizer as duas) prometeria um poder que não existe (`PoderesExigemSoOApp`, `poderes-exigem-so-o-app`). **Exige a metade de máquina** (`poderes-sem-maquina`): um poder é o que a máquina faz, e sem ela ninguém o exerce. E `porta` exige a permissão `entrar-no-jogo` em `permissoes` (`PortaSemEntrarNoJogo`, `porta-sem-entrar-no-jogo`): uma porta que ninguém pode abrir é recusada na instalação, com o nome, e não calada. |
| `maquina` | opcional: o caminho, dentro do pacote, do script que roda **na máquina de quem hospeda o jogo** (a metade de máquina, adiante). Fora do pacote é recusado (`MaquinaForaDoPacote`, `maquina-fora-do-pacote`); sem `poderes` também (`MaquinaSemPoderes`, `maquina-sem-poderes`); e sem a chave `server` também (`MaquinaSemServidor`, `maquina-sem-servidor`), porque quem pede ao dono da máquina que aceite é a metade de servidor. Numa API anterior à 8 é recusado como `poderes-pedem-api-nova` (a mesma recusa de `poderes`). |
| `state` | opcional: a versão do esquema dos seus dados, para as suas migrações. Lida, e ainda não usada pelo produto. |

**Chave desconhecida é recusada, e é de propósito.** `vesion` com um `s` a menos
instalaria calado um MOD sem versão, e você nunca ficaria sabendo. A recusa é a
única forma de retorno que este formato te dá.

**O build desta linha (a 2, em desenvolvimento) executa as APIs 8, 7 e 6.** Um `api` mais novo é recusado como
`api-too-new`. Um `api` de 3 a 5 é recusado como `api-too-old`, e a tela aponta
o Servidor 0.15 no launcher, que é a linha onde as APIs 3, 4 e 5 continuam
rodando. A 1 e a 2 rodavam na janela do produto, e isso não volta
([ADR 0049](adr/0049-um-mod-deixa-de-rodar-na-janela-do-produto.md)).

## A metade de cliente

Roda na máquina de quem entra, **num contexto QuickJS só dela**, e não na janela
do produto. Lá dentro não existem `document`, `window`, `fetch`, `localStorage`,
`indexedDB` nem o global do Tauri — e não existem porque ninguém os ligou, e não
porque alguém os apagou no começo do arquivo: o segundo caso se contorna.
Destruir o contexto destrói o MOD — temporizador, ouvinte e promessa atrasada
morrem junto, sem ele precisar cooperar. No navegador, o motor é o mesmo, num
Worker isolado: o contrato e os limites são os mesmos nos dois.

**Você declara, e quem desenha é o produto**, com `createElement` e
`textContent`. Não há forma que aceite HTML: o conteúdo vem de código de
terceiro, e interpretá-lo como marcação devolveria a página ao MOD por outro
caminho.

Um MOD da 6 desenha em **superfícies** (uma página, um diálogo, uma folha, um
aviso) e entra na interface do SEELE por **contribuições** (um ponto com nome,
como a navegação do servidor ou o cartão de uma pessoa):

```js
// cliente/main.js
const EU = "fulano/meu-mod";
let pagina = null;
let vezes = 0;

async function desenhar() {
  await pagina.montar([
    { forma: "titulo", chave: "t", dentro: "PLACAR" },
    { forma: "texto", chave: "v", dentro: `vezes: ${vezes}` },
    { forma: "botao", chave: "mais", dentro: "MAIS UM" },
  ]);
}

SeeleUI.aoEvento(async (evento) => {
  if (evento.nome === "acao" && evento.acao === "abrir-placar") {
    // Criar de novo com o mesmo id reabre a mesma página, sem recriar.
    pagina = await SeeleUI.superficies.criar({ id: "placar", tipo: "pagina", titulo: "Placar" });
    await desenhar();
  }
  if (evento.nome === "botao" && evento.chave === "mais") {
    // Canal 0: esta pergunta não é sobre canal nenhum.
    const resposta = await SeeleMods.request(EU, 0, { op: "contar" });
    vezes = resposta.vezes;
    await desenhar();
  }
});

// Uma entrada na navegação do servidor, que manda o evento `acao`.
SeeleUI.contribuicoes.registrar({
  ponto: "servidor.navegacao",
  modo: "adicionar",
  rotulo: "Placar",
  acaoPrincipal: "abrir-placar",
}).catch((falha) => console.warn(`a entrada não entrou: ${falha.erro}`));
```

### O que existe lá dentro

| | o que faz |
|---|---|
| `SeeleMods.request(id, canal, valor)` | pergunta à sua metade de servidor, que responde pelo `aoPedir`. `canal` é um identificador de canal, e `0` quer dizer «nenhum» — o número `0`, e não `null`. A promessa resolve com a resposta já lida do JSON (ver «O que volta de um pedido»). |
| `SeeleUI.retrato()` | o que a janela sabe, por lista branca: você, o servidor, quem está, as salas, os canais e a forma da casca (ver «O retrato»). |
| `SeeleUI.superficies.criar(descricao)` | uma janela do produto: `pagina`, `dialogo`, `folha` ou `aviso`. Devolve um punho, com `montar`, `atualizar`, `formulario.erros`, `classes`, `titulo`, `mostrar`, `ocultar`, `suja`, `fechar` e `descartar` (ver «As superfícies»). |
| `SeeleUI.superficies.avisar(texto, tom)` | um aviso de vida curta, sem montar nada; `tom` é `normal` (3,5 s) ou `erro` (10 s). |
| `SeeleUI.contribuicoes.registrar(pedido)` | entra num ponto da interface do SEELE, e resolve com `{ handle }`. `atualizar(handle, conteudo)` troca o conteúdo sem sair, e `revogar(handle)` sai (ver «As contribuições»). |
| `SeeleUI.tema(valores, { previa: true, superficie })` | a prévia de um tema, só nesta máquina e só enquanto a superfície estiver de pé. O tema que todos veem é o da metade de servidor (ver «O tema»). |
| `SeeleUI.aoEvento(fn)` | o que a pessoa faz no que o MOD desenhou, o que muda na tela e o aviso da metade de servidor voltam por aqui, sem número e sem resposta. Um ouvinte só, e o último vence. |
| `SeeleUI.pedaco(arquivo, inicio)` e `SeeleUI.soltar(arquivo)` | leem em pedaços o arquivo que alguém escolheu pela forma `arquivo`, e o devolvem. O MOD recebe identificador, papel e tamanho, e nunca o caminho. |
| `SeeleUI.enviar(arquivo, token)` | manda à sua metade de servidor uma imagem escolhida, até 10 MiB, por um fluxo próprio. A metade de servidor autoriza antes, com `volume.esperar`. **Só existe onde a plataforma tem o volume**: no navegador, não existe. |
| `SeeleUI.tunel.entrar(nome)` | **só existe para o MOD da API 8 que declara o poder `porta`**, e só no app. Abre a porta de quem joga até o jogo `nome` e resolve com o **número da porta em `127.0.0.1`** onde o jogo desta pessoa deve ligar (ver «A metade de máquina»). Só vale chamado de dentro do ouvinte de um `botao`, na mesma volta; as recusas que ele dá estão em «Os tetos e as recusas», na metade de máquina. |
| `SeeleUI.capacidades()` | o que este build oferece ao seu MOD, nesta plataforma. `estilos` e `classes` estão na lista como rótulos: nada pergunta por eles, e o estilo de um nó vale onde a forma dele monta. |
| `console` | cada chamada vira uma linha no `seele.log` da máquina que roda o MOD (ver «Quando o MOD não faz o que devia»). |
| `setTimeout`, `setInterval` e os dois `clear` | a fachada do relógio do anfitrião, com até 256 de pé; acima disso devolvem `0`, que é o «não deu». |

**O que a plataforma não tem não existe**, e não falha em tempo de execução. No
navegador, `SeeleUI.enviar` é `undefined`, e chamá-lo é um `TypeError` na
primeira linha. Pergunte a `SeeleUI.capacidades()` (ou `typeof SeeleUI.enviar`)
antes, e o mesmo arquivo roda nas duas.

**O que a 6 tirou, e onde mora agora:**

- **`SeeleUI.regiao`**, a faixa de 240 px: o desenho mora numa superfície, e as
  regiões da interface são pontos de contribuição;
- **`SeeleUI.cartoes`**: o cartão de uma pessoa é o ponto `pessoa.cartao`, com
  `alvo`;
- **`SeeleMods.snapshot()`**: é `SeeleUI.retrato()`;
- **`SeeleUI.tema(valores)` sem prévia**: rejeita com `sem-previa`. O tema
  publicado é da metade de servidor, `mundo.tema`.

**O que não existe, e onde mora:**

- **rede**: não há `fetch`. Rede é da metade de servidor, `mundo.buscar`;
- **o que precisa durar**: não há `localStorage` nem `indexedDB`. Vai para o
  servidor, pelo `aoPedir` e pelo `dados`, que é onde as permissões valem.

### O que volta de um pedido

`SeeleMods.request` resolve com o JSON que o seu `aoPedir` devolveu, já lido.
Quando a chamada não chegou a responder — o código lançou, passou de um teto, ou
o quintal não cabia —, ela resolve com o envelope do anfitrião:
`{ ok: false, error: "bridge-refused" }`, ou `{ ok: false, error: "quintal-cheio" }`
quando a gravação do `dados` passaria do teto. Escreva o seu protocolo de modo
que os dois não se confundam: um `erro` seu, por exemplo, e nunca `error`.

O pedido em si pode rejeitar, com um nome da lista fechada (ver «As recusas»):
`too-many-requests` com o nono pedido em voo, `mensagem-grande` acima de 12 KiB,
`sessao-encerrada` quando a sessão acabou antes da resposta.

### O retrato

`SeeleUI.retrato()` resolve com o que a janela sabe agora, copiado campo a campo
por lista branca:

| campo | o que é |
|---|---|
| `eu` | o seu id de pessoa, um número |
| `servidor.nome` | o nome do servidor |
| `pessoas` | quem está: `{ id, apelido, falando, mudo }` |
| `salas` | as salas de voz: `{ id, nome, limite, pessoas, minha }` |
| `canais` | os canais: `{ id, nome, aberto }` |
| `onde` | `computador` ou `celular` |
| `regioes` | as regiões e camadas que existem nesta casca agora |

**O que fica de fora fica de fora de propósito**: o veredito de conexão segura,
a topologia da rede, os aparelhos da máquina, a telemetria, as permissões, e
nunca o toque, a chamada, o código e a agenda da sala pessoal. Um campo novo do
estado do produto nasce fora do retrato até alguém o escrever na lista.

O retrato é uma leitura, e não uma assinatura: **o que muda chega por evento**
(`canal`, `sala`, `pessoas`, `fala`, `lugar`), e um MOD da 6 relê o retrato
quando um evento diz que vale a pena.

### Os eventos

Cada evento é um objeto com `nome`, entregue ao ouvinte de `SeeleUI.aoEvento`.
Os da interface que o MOD desenhou:

| `nome` | quando | o que traz |
|---|---|---|
| `campo` | digitou num `campo` ou `textoLongo` | `chave`, `valor` |
| `escolha` | mudou uma `escolha` | `chave`, `valor`, em texto |
| `numero` | mexeu num `numero` ou `deslizante` | `chave`, `valor`, um número |
| `marca` | mudou uma `marca` ou um `interruptor` | `chave`, `valor`, booleano |
| `cor` | mudou uma `cor` | `chave`, `valor` |
| `botao` | apertou um `botao` (com `confirmar`, só depois do sim) | `chave` |
| `submeter` | enviou um `formulario` que passou nas regras | `chave`, `valores` (número no `numero`, booleano na `marca`, texto no resto) |
| `aba` | escolheu uma aba | `chave` das `abas`, `valor` da aba |
| `arquivo` | escolheu, ou desistiu de escolher, pela forma `arquivo` | `chave`, `resultado` (`cancelado` ou `falhou`, com `porque`), e `arquivo: { id, papel, bytes }` |
| `traco` | arrastou numa `tela` | `chave`, `fase` (`comecou`, `moveu`, `terminou`, `cancelou`), `x`, `y`, e `alvo`: a `chave` da figura de cima onde o gesto começou |
| `midia` | o estado de uma mídia mudou | `chave`, `estado` (`tocando`, `pausada` e `terminou`, num som; `recusada` ou `falhou`, numa mídia que não pôde ser mostrada ou tocada — também num `retrato` e num `fundoDeMidia`: `recusada` pelo bolso de bytes ou por bytes que não são imagem, `falhou` quando a imagem não decodifica, um evento só por caso), `porque`; numa contribuição, `ponto` e `alvo` |
| `link` | apertou um `link` | `chave`, `aberto`, e `porque` quando não abriu |
| `tela` | o fundo de uma `tela` não carregou | `chave`, `estado: 'fundo-falhou'`, `porque` |
| `acao` | apertou a ação principal de uma contribuição | `acao`, e o alvo que o produto escreveu: `pessoa` no cartão de uma pessoa, `canal` numa ação de sala, **em texto**, e vazios quando não há |
| `fechar-pedido` | a pessoa pediu para fechar uma superfície suja | `superficie`, `porque`, `temAlteracoes` |
| `fechar` | uma superfície fechou | `superficie`, `porque`, e `descartou: true` quando a pessoa descartou o que não estava gravado |
| `contribuicao` | uma contribuição não coube, ou não tem onde entrar | `ponto`, `destino`, `recusados` e `amostra` (como a do `nos-recusados`), e `motivo`: `orcamento` (com `perfil`, `teto`, `tamanho`) ou `regiao-ausente` |

E os da tela, que dizem o que mudou sem o MOD perguntar:

| `nome` | o que traz |
|---|---|
| `canal` | `canal`: o canal aberto mudou |
| `sala` | `sala`, ou `null`: você entrou numa sala de voz ou saiu |
| `pessoas` | `entrou` ou `saiu`, com o id, um número: alguém entrou no servidor ou saiu |
| `fala` | `falando: [id]`, no máximo quatro vezes por segundo |
| `lugar` | `onde`, `regioes`, `largura`, `altura`, e `regiao` quando é o tamanho do lugar do MOD numa região ou camada (o `chamada.palco`: 0 × 0 escondido); sem `regiao`, o tamanho da sessão, **que só é dito quando a casca muda de forma**, e não ao chegar nem quando a janela muda de tamanho |
| `servidor` | `canal` e `carga`, já lida: o aviso da sua metade de servidor (`mundo.avisar`) |
| `tema` | `estado` (`aplicado`, `recusado` ou `nativo`), `previa`, e, recusado, `pares` e `regiao` (ver «O tema») |

**Nenhum evento carrega toque, chamada, código ou agenda.** O primeiro retrato de
uma sessão não é mudança: o MOD lê o retrato quando sobe.

### As superfícies

Uma superfície é uma janela do produto que o MOD declara e o produto monta. **A
casca é do produto**: o cabeçalho diz de qual MOD ela é, a saída é do produto,
com as palavras dele, e o foco entra na abertura, fica contido e volta a quem
abriu.

| `tipo` | o que é |
|---|---|
| `pagina` | uma vista inteira da sessão; `imersiva: true` esconde o resto |
| `dialogo` | uma janela por cima, `modal` (o padrão) ou não; um modal torna o resto inerte, e não só escuro |
| `folha` | uma folha ao lado (`lado: 'direita'` ou `'esquerda'`); até 3 à vista, empilhadas, e ela convive com o palco da chamada e com a página não imersiva |
| `aviso` | uma linha de vida curta; use `superficies.avisar` |

Sem `tipo`, `dialogo`. Um tipo que a 6 não tem é recusado como
`tipo-desconhecido`, com `tipoPedido` e os `tipos` que existem. O `painel` das
versões anteriores saiu: use `folha`.

A `descricao` do `criar` leva `id`, `tipo`, `titulo` e, quando servem, `modal`,
`tamanho: { largura }` (contida na área útil), `focoInicial` (a `chave` do campo
que recebe o foco), `lado`, `imersiva` e `visivel: false` (criar sem mostrar).

**O ciclo de vida, e o que cada passo guarda:**

- `criar` com um `id` que já está de pé **reabre aquela**, sem recriar e sem
  trocar o título (quem troca é `titulo(texto)`), e devolve um punho para ela.
- `montar(arvore)` declara o conteúdo inteiro. Chamar de novo substitui, e o
  produto reconcilia por `chave`: o que não mudou não é tocado, e quem está
  digitando não perde o foco.
- `atualizar(chave, arvore)` troca um nó e o que está sob ele, numa mensagem
  própria. É assim que uma ficha que não cabe nos 12 KiB de uma mensagem se
  monta: um esqueleto, e uma parte por vez. O teto que vale é o de nós da
  superfície.
- `ocultar()` tira da vista e mantém montada; `mostrar()` traz de volta.
- `fechar()` tira do documento e **guarda** o que ela tem: `montar` numa
  fechada vale, e aparece quando ela volta por `mostrar` ou por `criar`. O som
  de uma superfície fechada para; ocultar não para.
- `descartar()` acaba com ela, e o id fica livre.
- **Fechada continua de pé**: ela conta no teto de 12 superfícies do MOD. Quem
  não vai reabrir descarta.
- **A saída da sessão descarta tudo, sem perguntar** e sem mandar `fechar`.

Uma montagem cuja árvore tem o que não coube ou não serve (um teto, uma forma
fora do lugar, um estilo com uma parte errada, uma figura que a API não tem)
**monta o que coube** e rejeita com `nos-recusados`, que leva `recusados`, a
conta, e `amostra`: até oito `{ motivo, chave, forma, propriedade, tipo, classe }`,
com o que identifica cada uma. O `classes` rejeita do mesmo jeito.
**A fundura também é teto** (16 níveis numa superfície, 6 num cartão): o galho
que passa dela é recusado com o motivo `teto-de-fundura` e a chave dele. A
palavra dentro de um `texto` é um nível, como é um nó no orçamento: um `texto`
no último nível não tem onde pôr o que diz.
Uma forma que a API não tem rejeita com `forma-desconhecida`, com a `forma`, e
a tela fica como estava.

### Os formulários, e o que está sujo

Dentro de um `formulario`, cada forma de entrada declara `chave`, `rotulo` e
`valor`, e pode declarar:

- **as regras**, que o produto confere antes de deixar o `submeter` sair:
  `obrigatorio`, `tamanhoMaximo`, e, só no `numero`, `minimo`, `maximo` e
  `inteiro`. A frase é do produto — «Preencha este campo.», «Use até 40
  caracteres; este tem 41.», «Use um número de 1 a 20.», «Use um número a partir
  de 18.», «Use um número inteiro.» —, o campo é marcado, e o foco vai a ele.
  Numa `marca`, `obrigatorio` não age;
- **`erro`**, a frase do MOD, em qualquer forma de entrada, inclusive `cor` e
  `escolha`;
- **`inicial`**, o valor gravado.

O `formulario` declara `primario` e `secundario` (o produto monta os dois
botões: o primário envia todos os valores juntos, e o secundário sai) e
`emProgresso` (o primário ignora o segundo clique). Qualquer `botao` pode
declarar `confirmar`: o produto pergunta com as palavras dele, e o evento
`botao` só chega depois do sim.

A recusa da sua metade de servidor volta ao campo que a causou por
`punho.formulario.erros({ chave: "frase" })`, um campo ou vários; `{}` limpa. Uma
chave que não é de um campo à vista é recusada como `chave-desconhecida` — o
campo de uma aba que não está aberta não está à vista.

**O sujo é do produto.** Um campo cujo valor difere do `inicial` suja a
superfície, sem o MOD chamar `suja`, e voltar ao gravado a limpa. `suja(true)`
soma o que o MOD sabe e o produto não vê. **Uma superfície suja sempre pergunta
antes de fechar**, inclusive quando quem pede é o próprio MOD, e a resposta da
pessoa vence: ao descartar, o MOD recebe `fechar` com `descartou: true`.

Duas coisas que um MOD precisa saber para não cair nisso:

- **Sem `inicial`, o inicial é o valor da primeira montagem do campo**, pela
  chave, e não muda quando o MOD regrava o valor. Um controle que o MOD regrava
  — a trilha que a pessoa escolheu e o MOD já gravou, a fórmula que ele
  recalcula — declara `inicial` igual ao `valor`. Sem isso, depois da primeira
  gravação, a superfície fica suja para sempre, e fechar pergunta «sem gravar?»
  de algo gravado.
- **Para fechar sem perguntar**, o MOD declara o gravado como `inicial`, ou
  fecha com `fechar("descartar")`, que fecha sem a pergunta: é o MOD dizendo
  que o rascunho pode sair.

### As formas e as figuras

As trinta formas da API estão em `formas`, no `api/v6.json`, e cada uma vira um
elemento do produto. As de composição (`caixa`, `pilha`, `grade`, `linha`,
`rolagem`, `lista`, `acoes`) levam os filhos em `dentro`; as de texto (`titulo`,
`texto`, `item`, `distintivo`) levam o texto em `dentro`. Um texto solto dentro
de `dentro` também vale.

| forma | o que mais declara |
|---|---|
| `campo`, `textoLongo` | `rotulo`, `valor`, `inicial`, `erro`, as regras; `textoLongo` aceita `linhas` |
| `escolha` | `opcoes: [{ valor, dentro }]` |
| `numero`, `deslizante` | `minimo`, `maximo`; o deslizante mostra o valor ao lado |
| `marca`, `interruptor` | `valor` booleano; o interruptor tem o papel de interruptor para quem usa leitor de tela, e os dois falam `marca` |
| `cor` | `valor` em `#rrggbb`, com a caixa do hexadecimal ao lado |
| `botao` | `desligado`, `variante` (`primaria`, `secundaria`, `perigo`, `discreta`), `emProgresso`, `confirmar` |
| `arquivo` | `finalidade`, `tipos` (`imagem`, `som`), `limiteDeBytes`, `desligado`; o seletor é do sistema, e o MOD nunca vê o caminho |
| `abas`, `aba` | as `aba` dentro de `abas`, com `rotulo`; `valor` nas `abas` escolhe a aberta, e só ela é montada |
| `acoes` | `fixas: true` leva os botões ao rodapé do produto, numa superfície |
| `link` | `endereco`, que fica à vista; o produto abre fora, e só `http` e `https` |
| `midia` | `fonte` (um caminho de `arquivos`), ou `doServidor`; `tocando`, `repetir`, `descricao` |
| `retrato` | `inicial` (uma letra), `formato` (`circulo`, `quadrado`, `arredondado`), `fonte`, `doServidor` ou a citação `daPessoa` |
| `tela` | `largura`, `altura`, `figuras`, `fundo` |

Em toda forma, `chave` (o que faz a atualização ser incremental e volta nos
eventos), `estilo`, `classe` e `nomeAcessivel`.

**As figuras** de uma `tela` são `retangulo`, `circulo`, `texto` e `linha`. Uma
figura com `chave` é pegável: o `traco` diz qual foi pega, e a última declarada
é a de cima. Uma `linha` não é pegável. Um tipo que a API não tem é contado como
recusa, e não vira retângulo por conveniência. Até 256 figuras por tela, e quem
manda é a mensagem: com `chave` e `cor` cabem 155 em 12 KiB.

Uma `caixa` aceita `fundoDeMidia`, com a mesma origem de uma mídia
(`{ doServidor }`, `{ fonte }`) ou a citação da capa de uma pessoa (ver «A
capa»).

### As contribuições

Uma contribuição entra num **ponto com nome** da interface do SEELE — nunca num
seletor de CSS, num nó do documento ou em HTML de texto. Os pontos e os modos de
cada um estão em `contribuicoes.pontos`, no `api/v6.json`:

| ponto | modos | o que é |
|---|---|---|
| `servidor.navegacao` | `substituir`, `adicionar` | a lista do servidor; `listarNaBarra: false` tira a entrada da lista de quem não tem uso para ela (no celular, da aba SERVIDOR) |
| `canal.cabecalho` | `substituir`, `adicionar` | o cabeçalho do canal |
| `canal.item` | `adicionar` | um canal da lista, por `alvo` |
| `compositor.ferramentas` | `adicionar` | ao lado do campo de mensagem |
| `chamada.palco` | `substituir`, `adicionar` | uma camada sobre a chamada (ver «A chamada») |
| `servidor.aparencia` | `substituir` | a aparência do servidor: na 6, o MOD de janela não a pinta por ela (ver «O tema») |
| `pessoa.identidade`, `pessoa.detalhes`, `pessoa.acoes` | `adicionar` | na pessoa, por `alvo` |
| `pessoa.cartao` | `substituir`, `adicionar` | o cartão da pessoa na lista, por `alvo`, numa gramática menor: nada que receba foco ou clique |
| `pessoa.avatar`, `pessoa.capa` | `substituir` | a foto e a capa da pessoa, por `alvo`, com `conteudo: { doServidor }` |
| `sala.acoes` | `adicionar` | uma ação na sala de voz, por `alvo` |

O `pedido` do `registrar` leva `ponto`, `modo`, e, quando servem, `alvo` (o id
da pessoa, do canal ou da sala), `conteudo` (a árvore, ou a mídia nos pontos de
imagem), `prioridade` (de -100 a 100: ordena duas substituições do mesmo ponto,
e quem escolhe é a pessoa), `rotulo`, `nomeAcessivel` e `acaoPrincipal`. A
`acaoPrincipal` é o nome que volta no evento `acao` quando a pessoa aperta, com o
alvo que o **produto** escreveu: trocar o que se desenha não troca a identidade.
`classes` são de superfície: numa contribuição, elas não pintam nada, e o
evento `contribuicao` conta isso (`classes-fora-da-superficie`).

- **`atualizar(handle, conteudo)` troca sem sair**, e a linha não volta à nativa
  no meio. Ele resolve com `true`, ou com `false` quando o punho não está mais
  de pé (você revogou, ou a instância acabou) — e aí é registrar de novo.
  Registrar de novo o que está de pé é `contribuicao-ja-existe`.
- **O produto não revoga por presença.** A contribuição de uma pessoa que saiu
  do servidor continua de pé até o MOD a revogar; o evento `pessoas` com `saiu`
  é a hora.
- **Os tetos**: 128 contribuições sem alvo de pé, e 256 alvos por ponto. A
  recusa `teto-de-alvos` leva `teto`.
- **Uma contribuição para uma região que a casca não tem** (no celular, o
  `status.barra`) é aceita e não aparece: o MOD recebe o evento `contribuicao`
  com `regiao-ausente`, e a aba DIAGNÓSTICO conta.
- **O que o produto garante**: o apelido verificado fica à vista, num lugar que
  o MOD não cobre; uma região substituída continua carregando os itens do
  núcleo; ao revogar, a apresentação nativa é recalculada; «usar apresentação
  padrão» existe no controle nativo de MODs.

### A capa, e a citação

A capa de uma pessoa é o ponto `pessoa.capa`, em `substituir`, por `alvo`, com
`conteudo: { doServidor }` — a mesma forma do `pessoa.avatar`. O produto a pinta
no cartão da chamada, no fundo do operador e no «SEU PERFIL», sobre placas que
mantêm o texto e os controles legíveis. Animada até seis ao mesmo tempo,
escolhidas pela vista; com a preferência de movimento reduzido do sistema, com a
janela oculta ou fora da vista, um quadro parado. A capa de servidor é reduzida
ao tamanho de uso antes de chegar à janela.

**Citar, em vez de carregar de novo.** `fundoDeMidia: { daPessoa: 'capa' }` numa
`caixa` e `retrato({ daPessoa: 'avatar' })` usam a imagem que a contribuição da
pessoa já carregou — sem pedido novo, e contada uma vez. Num cartão, a pessoa é
a do destino. Com `pessoa: id`, é a que o MOD nomeia, em qualquer destino,
numa superfície também, se ela estiver na sessão pelo retrato. Fora da sessão,
ou com um id que não é id, o fundo fica liso e o retrato na inicial, sem recusa.
Na lista de pessoas, a capa citada fica parada; numa folha, ela anima. Quando a
pessoa troca a foto ou a capa e o MOD faz `atualizar` na contribuição, quem cita
mostra a nova sem remontar.

### Os estilos e as classes

`estilo` numa forma declara propriedades por categoria, **validadas uma a uma e
montadas de partes conferidas — nunca texto de CSS**: cor e transparência
(`cor`, `fundo`, `opacidade`), preenchimento e gradiente, borda e raio, sombra,
tipografia, layout (`direcao`, `alinhar`, `distribuir`, `quebra`, `crescer`,
`encolher`, `base`, `intervalo`, `colunas`, `largura`, `altura` e os mínimos e
máximos), transformação (`girar`, `escalar`, `mover`), recorte, transição e
animação (`nenhuma`, `pulso`, `aurora`, `brilho`, `flutuar`). Uma cor é
`#rgb`, `#rrggbb`, `#rrggbbaa`, ou um token do tema: `{ tema: 'acento' }`, que
acompanha o tema em vigor.

**O que não serve é contado, e não some calado.** Uma propriedade que o
validador não conhece, ou um valor que não serve, é contado como recusa — e,
numa propriedade composta (`borda`, `sombra`, `gradiente`, `mover`, `transicao`,
`animacao`), também a parte declarada que não serve: a propriedade sai com o
padrão naquela parte, e a recusa chega ao MOD como `nos-recusados`, com a
propriedade na amostra. Um número fora do
intervalo não é erro: ele é preso no limite.

**Classes** são de superfície: `punho.classes({ nome: { base, sobre, foco,
ativo, desligado, invalido, escolhido, consultas } })`, e o nó pede a classe com
`classe`. O seletor é escrito pelo produto e preso àquela superfície: duas
superfícies com a classe `cartao` não se alcançam, e nenhuma alcança o SEELE. Os
estados são seletores — sem eles, uma cor de passar o mouse custaria um evento
por movimento. `consultas: [{ ateLargura, estilo }]` aplica o estilo até uma
largura, **a da superfície**, e não a da janela: um diálogo de 400 px numa
janela de 1400 é estreito.

**Fora do alcance de qualquer estilo**: `url(...)`, `@import`, HTML ativo,
seletor que alcance fora da superfície, `keyframes` do MOD e família de tipo
livre. O movimento reduzido do sistema vale onde quer que uma contribuição de
MOD apareça, e, no celular, nenhum estilo desce um controle abaixo de 44 px nem
um campo abaixo de 16 px de letra.

### O tema

O tema é de **duas portas**, uma de cada lado:

- **a do servidor**: o manifesto declara `"tema": true`, e a metade de servidor
  publica o tema com `mundo.tema(valores)` (ver «A metade de servidor»). Um tema
  por servidor: o primeiro MOD com a porta, enquanto estiver ligado. A tela de
  aceite mostra a porta.
- **a de quem vê**: o ponto `servidor.aparencia`. «O SEELE desenha» tira o tema
  daquele servidor nesta máquina.

A janela do MOD só faz a **prévia**:
`SeeleUI.tema(valores, { previa: true, superficie: id })` pinta só nesta máquina,
com o selo «prévia não publicada», e sai quando a superfície fecha ou com
`SeeleUI.tema(null, { previa: true })`. Uma prévia recusada rejeita com
`contraste`, com os `pares`.

**As chaves** são uma lista fechada de nomes do contrato, e nunca nomes de CSS:
`fundo`, `painel`, `texto`, `apagado`, `acento`, `borda` (cores `#rrggbb`),
`densidade` (`compacta`, `confortavel`), `fonte` (`mono`, `sans`),
`arredondamento` (um inteiro de 0 a 24), `brilho` (booleano), `modo` (`claro`, `escuro`) e
`regioes` (`{ '<região>': { fundo, texto, apagado, acento, borda } }`, as oito
regiões, conferidas uma a uma). O `modo` tem de concordar com o fundo: claro de
0,18 de luminância relativa para cima. Uma chave fora da lista, ou um valor
fora da forma, é recusado pelo nome (`chave-desconhecida`, `valor-invalido`,
`regiao-desconhecida`), com a `chave`.

**O contraste é conferido pela casca, ao aplicar**, em todos os pares que o
produto desenha: o texto e o acento a 4,5:1 sobre o fundo e o painel, o apagado
a 3:1. O `acentoFraco` e o `apagadoSecundario` são do produto: derivados do seu
tema, e não chaves dele. As cores de estado do produto — o fósforo, o azul e o
vermelho, e os `-fraco` — também: sob um tema, a casca as deriva das suas
superfícies; quando nenhuma luminosidade serve às duas, o tema é recusado com o
par, `{ frente: 'fosforo' | 'azul' | 'vermelho', fundo: 'painel', razao, piso }`
— e, para o segundo apagado, `frente: 'apagadoSecundario'`, a 3:1. Cada janela
diz o desfecho pelo evento `tema`: `aplicado`, `recusado` (com os `pares`, e a
`regiao` quando quem recusou foi uma região) ou `nativo` (quem vê escolheu «o
SEELE desenha»).

**Nenhum item do núcleo de confiança é alcançado pelo tema**: o MUDO, o fone, o
«no ar», o SAIR e o veredito da conexão leem tokens próprios.

**A função de referência.** Para saber o que a casca vai derivar e recusar antes
de publicar, sem uma janela aberta, este é o cálculo dela, por inteiro. Ele não
depende de nada: copie o bloco, e chame
`tonsDoTema(seuTema, corDoProduto)`. O resultado é `{ tons, pares }`: os tokens
que a casca escreveria, e os pares que reprovam. A bateria executa esta cópia
contra a da casca, tema a tema, e o build reprova se elas divergirem.

```js
// ---- tonsDoTema: começo ----
//
// A função de referência do `api/v6.json` (`tema.derivados-pelo-produto`): os
// tons que a casca deriva de um tema e os pares que ela recusa. É uma cópia do
// que a casca faz (`apps/seele-app/ui/base.js`), e o guarda
// `apps/seele-app/bancada/tons-do-tema.cjs` a executa: para cada tema de uma
// lista, esta cópia e a da casca dão o mesmo resultado, e as cores abaixo são
// as de `apps/seele-app/ui/tokens.css`.

/** As cores do produto que a derivação lê, de `tokens.css`. */
const CORES_DO_PRODUTO = Object.freeze({
  "--seele-negro-absoluto": "#000000",
  "--seele-osso": "#eaeaea",
  "--seele-laranja-nerv": "#f47732",
  "--seele-linha": "#252525",
  "--seele-negro-painel": "#000000",
  "--seele-rotulo-painel": "#8a8a8a",
  "--seele-laranja-fraco": "#331704",
  "--seele-fosforo": "#6bffb6",
  "--seele-fosforo-fraco": "#06220f",
  "--seele-padrao-azul": "#2f8fe0",
  "--seele-azul-fraco": "#04182b",
  "--seele-vermelho-alerta": "#ff6262",
  "--seele-vermelho-fraco": "#2f1413",
  "--seele-osso-apagado": "#b0b0b0",
});

/** A cor de um token do produto, para o segundo argumento de `tonsDoTema`. */
const corDoProduto = (token) => CORES_DO_PRODUTO[token] ?? "";

const COR_DO_TEMA = /^#[0-9a-f]{6}$/i;

const TEMA_DA_API = Object.freeze({
  fundo: "--seele-negro-absoluto",
  texto: "--seele-osso",
  acento: "--seele-laranja-nerv",
  borda: "--seele-linha",
  // **Painel e apagado entraram porque faltavam.** Um MOD de tema guardava os
  // seis no servidor e só conseguia aplicar quatro, e o que ele mostrava na
  // tela era um aviso de que os outros dois «aguardam suporte». Um aviso de
  // indisponibilidade no lugar de um comportamento é a forma mais barata de
  // não entregar uma coisa, e é por isso que ela não vale.
  painel: "--seele-negro-painel",
  apagado: "--seele-rotulo-painel",
});

function luminanciaRelativa(cor) {
  const canais = [1, 3, 5].map((i) => {
    const c = parseInt(cor.slice(i, i + 2), 16) / 255;
    return c <= 0.03928 ? c / 12.92 : ((c + 0.055) / 1.055) ** 2.4;
  });
  return 0.2126 * canais[0] + 0.7152 * canais[1] + 0.0722 * canais[2];
}

function contrasteEntre(a, b) {
  const [claro, escuro] = [luminanciaRelativa(a), luminanciaRelativa(b)].sort((x, y) => y - x);
  return (claro + 0.05) / (escuro + 0.05);
}

const PARES_DO_TEMA = Object.freeze([
  ["texto", "fundo", 4.5],
  ["texto", "painel", 4.5],
  ["apagado", "fundo", 3],
  ["apagado", "painel", 3],
  ["acento", "fundo", 4.5],
  ["acento", "painel", 4.5],
]);

/**
 * **As cores de estado do produto**, e o preenchimento de cada uma. O MOD não
 * as escolhe; sob um tema, o produto as deriva das superfícies dele (achado da
 * F8, 09/10: o fósforo de «juli fala» sumia sobre um painel claro).
 */
const CORES_DE_ESTADO = Object.freeze([
  { nome: "fosforo", token: "--seele-fosforo", fraco: "--seele-fosforo-fraco" },
  { nome: "azul", token: "--seele-padrao-azul", fraco: "--seele-azul-fraco" },
  { nome: "vermelho", token: "--seele-vermelho-alerta", fraco: "--seele-vermelho-fraco" },
]);

/** O quanto um preenchimento se afasta do fundo, e quanto do croma da cor ele leva. */
const PASSO_DO_PREENCHIMENTO = 0.122;
const CROMA_DO_PREENCHIMENTO = 0.23;

/** Uma cor `#rrggbb` em OKLab `[L, a, b]`. */
function oklabDe(hex) {
  const [r, g, b] = [1, 3, 5].map((i) => {
    const c = parseInt(hex.slice(i, i + 2), 16) / 255;
    return c <= 0.04045 ? c / 12.92 : ((c + 0.055) / 1.055) ** 2.4;
  });
  const l = Math.cbrt(0.4122214708 * r + 0.5363325363 * g + 0.0514459929 * b);
  const m = Math.cbrt(0.2119034982 * r + 0.6806995451 * g + 0.1073969566 * b);
  const s = Math.cbrt(0.0883024619 * r + 0.2817188376 * g + 0.6299787005 * b);
  return [
    0.2104542553 * l + 0.793617785 * m - 0.0040720468 * s,
    1.9779984951 * l - 2.428592205 * m + 0.4505937099 * s,
    0.0259040371 * l + 0.7827717662 * m - 0.808675766 * s,
  ];
}

/** OKLab para luz linear `[r, g, b]`, sem recortar. */
function luzDeOklab(L, a, b) {
  const l = (L + 0.3963377774 * a + 0.2158037573 * b) ** 3;
  const m = (L - 0.1055613458 * a - 0.0638541728 * b) ** 3;
  const s = (L - 0.0894841775 * a - 1.291485548 * b) ** 3;
  return [
    4.0767416621 * l - 3.3077115913 * m + 0.2309699292 * s,
    -1.2684380046 * l + 2.6097574011 * m - 0.3413193965 * s,
    -0.0041960863 * l - 0.7034186147 * m + 1.707614701 * s,
  ];
}

/**
 * A cor de luminosidade `L`, croma `C` e matiz `h` (OKLCH), como `#rrggbb`. O
 * croma **recua** até a cor caber no sRGB, e a luminosidade e o matiz ficam.
 */
function hexDeOklch(L, C, h) {
  const cabe = (c) => luzDeOklab(L, c * Math.cos(h), c * Math.sin(h)).every((v) => v >= -1e-6 && v <= 1 + 1e-6);
  let croma = C;
  if (!cabe(C)) {
    let baixo = 0;
    let alto = C;
    for (let i = 0; i < 30; i += 1) {
      const meio = (baixo + alto) / 2;
      if (cabe(meio)) baixo = meio;
      else alto = meio;
    }
    croma = baixo;
  }
  return "#" + luzDeOklab(L, croma * Math.cos(h), croma * Math.sin(h))
    .map((v) => {
      const x = Math.min(1, Math.max(0, v));
      const c = x <= 0.0031308 ? 12.92 * x : 1.055 * x ** (1 / 2.4) - 0.055;
      return Math.round(c * 255).toString(16).padStart(2, "0");
    })
    .join("");
}

/**
 * **O preenchimento de uma cor sobre um fundo**: o matiz da cor, uma parte do
 * croma dela, e a luminosidade do fundo um passo na direção da cor. O passo
 * **encolhe** até a cor alcançar `piso` sobre o preenchimento — num tema claro,
 * um preenchimento escuro demais faria o texto da cor sumir nele.
 */
function preenchimentoDe(cor, fundo, piso = 4.5) {
  const [L, a, b] = oklabDe(cor);
  const [L0] = oklabDe(fundo);
  const C = Math.hypot(a, b);
  const h = Math.atan2(b, a);
  const sentido = L >= L0 ? 1 : -1;
  for (let passo = PASSO_DO_PREENCHIMENTO; passo > 0; passo -= 0.004) {
    const candidato = hexDeOklch(L0 + sentido * passo, C * CROMA_DO_PREENCHIMENTO, h);
    if (contrasteEntre(cor, candidato) >= piso) return candidato;
  }
  return hexDeOklch(L0, C * CROMA_DO_PREENCHIMENTO, h);
}

/**
 * **Uma cor do produto sob as superfícies de um tema**, ou `null` quando
 * nenhuma luminosidade serve. Mantida, se já alcança `piso` sobre o fundo e o
 * painel; senão, o matiz fica e a luminosidade se move para longe das
 * superfícies, até o ponto mais próximo da cor do produto que alcança. A cor
 * sobre o próprio preenchimento não é conferida aqui: é o preenchimento que se
 * aproxima do fundo até ela alcançar (`preenchimentoDe`).
 */
function corDeEstadoPara(cor, fundo, painel, piso = 4.5) {
  const alcanca = (c) => contrasteEntre(c, fundo) >= piso && contrasteEntre(c, painel) >= piso;
  if (alcanca(cor)) return cor;
  const [L, a, b] = oklabDe(cor);
  const C = Math.hypot(a, b);
  const h = Math.atan2(b, a);
  const claras = (oklabDe(fundo)[0] + oklabDe(painel)[0]) / 2 > 0.6;
  let perto = L;
  let longe = claras ? 0 : 1;
  let achada = null;
  for (let i = 0; i < 40; i += 1) {
    const meio = (perto + longe) / 2;
    const candidata = hexDeOklch(meio, C, h);
    if (alcanca(candidata)) {
      achada = candidata;
      longe = meio;
    } else {
      perto = meio;
    }
  }
  return achada;
}

/**
 * **Os tons que o produto deriva do tema em vigor, e os pares que reprovam**
 * (rascunho da API 6, §9; tarefa 17e). `emVigor` é o tema dos MODs; o que ele
 * não define vem de `daRaiz`, a cor do produto. Sem nenhuma cor no tema, nada é
 * derivado: o produto fica como é.
 *
 * Devolve `{ tons: { token: cor }, pares: [{ frente, fundo, razao, piso }] }`,
 * com os pares que não alcançam o piso — os do MOD e, quando nenhuma
 * luminosidade serve, os das cores de estado.
 */
function tonsDoTema(emVigor, daRaiz) {
  const cores = ["fundo", "painel", "texto", "apagado", "acento"];
  if (!cores.some((nome) => typeof emVigor[nome] === "string")) return { tons: {}, pares: [] };
  const c = {};
  for (const nome of cores) c[nome] = String(emVigor[nome] ?? daRaiz(TEMA_DA_API[nome])).toLowerCase();
  const pares = [];
  for (const [frente, fundo, piso] of PARES_DO_TEMA) {
    const razao = contrasteEntre(c[frente], c[fundo]);
    if (razao < piso) pares.push({ frente, fundo, razao: Number(razao.toFixed(2)), piso });
  }
  // Uma cor que não carregou não vira recusa, nem tom: sem cor medida não há
  // medida, e derivar dela seria escrever na sessão um valor que não é cor.
  const medida = (...cs) => cs.every((cor) => COR_DO_TEMA.test(cor));
  const raiz = (token) => String(daRaiz(token)).toLowerCase();
  // **As superfícies exatas do produto recebem os tons do produto.** Os
  // `-fraco` foram escolhidos à mão sobre elas, e nenhuma fórmula os reproduz
  // (ΔE2000 de 3,2 a 4,2, medido na 17e): com o fundo e o painel do produto, o
  // preenchimento de uma cor do produto é o token dela, ΔE 0. Só o que depende
  // de uma cor que o MOD mudou é derivado.
  const doProduto = c.fundo === raiz(TEMA_DA_API.fundo) && c.painel === raiz(TEMA_DA_API.painel);
  const preenchimento = (cor, corDoProduto, tokenDoProduto) =>
    doProduto && cor === corDoProduto && medida(raiz(tokenDoProduto))
      ? raiz(tokenDoProduto)
      : preenchimentoDe(cor, c.fundo);
  const tons = {};
  if (medida(c.acento, c.fundo)) {
    tons["--seele-laranja-fraco"] = preenchimento(c.acento, raiz(TEMA_DA_API.acento), "--seele-laranja-fraco");
  }
  const superficies = medida(c.fundo, c.painel);
  for (const estado of CORES_DE_ESTADO) {
    const cor = raiz(estado.token);
    if (!superficies || !medida(cor)) continue;
    const derivada = corDeEstadoPara(cor, c.fundo, c.painel);
    if (!derivada) {
      pares.push({ frente: estado.nome, fundo: "painel", razao: Number(contrasteEntre(cor, c.painel).toFixed(2)), piso: 4.5 });
      continue;
    }
    tons[estado.token] = derivada;
    tons[estado.fraco] = preenchimento(derivada, cor, estado.fraco);
  }
  // **O segundo apagado do produto** (`--seele-osso-apagado`), que nenhum tema
  // escreve: no piso dos apagados, 3:1 sobre o fundo e o painel.
  // Recusado, o par leva o nome que o `v6.json` dá ao derivado
  // (`apagadoSecundario`, em `tema.derivados-pelo-produto.nomes`).
  const apagado = raiz("--seele-osso-apagado");
  if (superficies && medida(apagado)) {
    const segundo = corDeEstadoPara(apagado, c.fundo, c.painel, 3);
    if (segundo) tons["--seele-osso-apagado"] = segundo;
    else pares.push({ frente: "apagadoSecundario", fundo: "painel", razao: Number(contrasteEntre(apagado, c.painel).toFixed(2)), piso: 3 });
  }
  return { tons, pares };
}

// Por exemplo: tonsDoTema({ fundo: "#f4f1ea", painel: "#ffffff", texto: "#1d1b16" }, corDoProduto)
// → { tons: { "--seele-laranja-fraco": "#…", … }, pares: [{ frente, fundo, razao, piso }, …] }
//
// ---- tonsDoTema: fim ----
```

### O som

Uma `midia` com som é tocada pelo produto, e o botão TOCAR/PAUSAR é dele (num
cartão, nenhum).

- **`tocando` vale quando muda.** Um som que a pessoa pausou não volta sozinho
  no redesenho seguinte que o declare `tocando: true`. A exceção é o som que
  terminou: `tocando: true` sobre ele toca de novo, do começo.
- **`repetir: true` toca em laço**, sem redesenho, e o MOD não recebe
  `terminou` dele.
- **Fora da tela, a declaração não liga o som**: ela é recusada, com o motivo,
  e um som que tocava ali é pausado. Fechar uma superfície cala o som dela;
  ocultar, não. O palco da chamada escondido continua montado, e soando.
- No celular, som de MOD só com a tela acesa.

### A chamada

O ponto `chamada.palco` é uma camada sobre a chamada, com a grade de vídeo do
produto por baixo. A substituição escolhida fica no miolo, e os acréscimos numa
faixa abaixo dele. Ele começa escondido: VER O PALCO, na barra da chamada, o
traz; VER A GRADE o devolve, sem desmontar; sair da sala o desmonta. Cada janela
monta o que o MOD dela declarar, e o pedido feito do palco leva `contexto.sala`.
O MOD sabe o tamanho pelo evento `lugar`, com `regiao: 'chamada.palco'`, ao
chegar e a cada mudança — 0 × 0 escondido. O MOD que não quer tocar escondido
lê o lugar e pausa.

### As recusas

**O erro que o MOD recebe é um nome de uma lista fechada.** A frase fica na
tela, e o motivo no `seele.log`. O prelúdio rejeita com um `Error` cujo
`message` e `erro` são o nome, e que leva os outros campos como propriedades:

```js
try {
  await SeeleUI.enviar(arquivo.id, token);
} catch (falha) {
  if (falha.erro === "tipo-recusado") dizer(`não é imagem que o SEELE lê: ${falha.detectado}`);
  else dizer(falha.erro);
}
```

| grupo | os nomes |
|---|---|
| ponte | `falhou` (o genérico), `too-many-requests` (leva `teto`), `fila-cheia`, `mensagem-grande` (leva `tipoPedido`, `tamanho`, `teto`), `tipo-desconhecido` (leva `tipoPedido`), `sessao-encerrada` |
| volume | `sem-espera`, `tipo-recusado` (leva `detectado`), `formato-desconhecido`, `arquivo-grande-demais`, `nao-gravei`, `incompleto`, `sessao-encerrada` |
| arquivo | `banco-indisponivel`, `arquivo-nao-esta-de-pe`, `arquivo-de-outro-mod`, `arquivo-ilegivel`, `caminho-invalido`, `papel-nao-serve` (leva `papel`), `arquivos-demais-de-pe` |
| superfícies | `tipo-desconhecido` (leva `tipoPedido` e `tipos`), `chave-desconhecida` (leva `chave`), `orcamento` (leva `perfil`, `teto`, `tamanho`), `forma-desconhecida` (leva `forma`), `nos-recusados` (leva `recusados` e `amostra`), `superficie-desconhecida` (leva `superficie`), `sem-previa`, `contraste` (leva `pares`), `teto-de-folhas` e `teto-de-superficies` (levam `teto`) |
| contribuições | `ponto-desconhecido` (leva `pontoPedido` e `pontos`, os que a API tem), `modo-recusado`, `sem-alvo`, `conteudo-invalido`, `teto-de-contribuicoes`, `teto-de-alvos` (leva `teto`), `contribuicao-ja-existe`, `contribuicao-de-outro-mod` |
| tema, no servidor | `sem-porta-de-tema`, `tema-de-outro-mod`, `tema-invalido`, `tema-grande-demais`, `chave-desconhecida`, `valor-invalido`, `regiao-desconhecida` (em `mundo.tema`, como valor, e não como exceção) |

**Um nome fora da lista chega como `falhou`, sem campos.** Não há outro
caminho: a janela confere a lista na fronteira.

### Os tetos da janela

| | quanto |
|---|---|
| uma mensagem (cada chamada) | 12 KiB em UTF-8; acima, `mensagem-grande` |
| pedidos em voo | 8; o nono, `too-many-requests` |
| superfícies de pé | 12 por MOD; avisos, 4; folhas à vista, 3 |
| numa superfície | 4096 nós, 16 de fundura, 128 campos, 4 telas, 24 mídias |
| mídia numa superfície | um bolso de 24 MiB que a `midia`, o `retrato` e o `fundoDeMidia` dividem; a `midia`, até 10 MiB dele. Numa região, 10 MiB |
| figuras numa tela | 256, até o teto da mensagem |
| contribuições | 128 sem alvo de pé; 256 alvos por ponto |

## A metade de servidor

Roda num QuickJS da máquina de quem hospeda, um por MOD. Você define duas
funções, e o produto põe quatro objetos globais.

```js
// servidor/main.js
globalThis.aoPedir = (contexto, pedido) => {
  const quem = JSON.parse(contexto);
  const o_que = JSON.parse(pedido);
  if (o_que.op !== "contar") {
    return JSON.stringify({ erro: `operação desconhecida: ${o_que.op}` });
  }
  const vezes = Number(dados.vezes ?? "0") + 1;
  dados.vezes = String(vezes);
  // As janelas deste MOD ficam sabendo, pelo evento `servidor`.
  mundo.avisar({ carga: { vezes } });
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

- **`contexto` é o que o servidor sabe**: `person`, quem pediu, **em texto**;
  `channel`, o canal, ou `null` quando a janela mandou `0`; `sala`, a sala de
  voz em que a pessoa está segundo a presença que o servidor mantém, ou `null`;
  `admin`, se essa pessoa administra o servidor; e `write`, se ela pode escrever
  **naquele canal** (a permissão de escrever e o papel mínimo do canal, a mesma
  pergunta que o produto faz para publicar ali) — ou, no pedido sem canal, se
  ela pode escrever nos canais em geral. Um pedido num canal que a pessoa não lê
  nem chega ao `aoPedir`. **A autorização sai daqui, e nunca do pedido**: um MOD
  que confiasse num campo do pedido deixaria quem escreve a chamada escolher a
  própria permissão.
- **`pedido` é o que a janela mandou**, o terceiro argumento do `request`.
- **Uma resposta acima de 1 MiB é recusada**, e o `dados` só é gravado quando a
  chamada termina bem — como no `aoAcontecer`.

O `person` chega em texto, e o `id` das pessoas no retrato da janela é um
número: compare com `String(id)`.

### `aoAcontecer(momento, carga)`

`momento` é o nome do evento — **o mesmo nome que o protocolo usa**, para que um
relatório de defeito que diga `MessageReceived` ache a mesma palavra no
protocolo, no `api/v6.json` e no seu código.

**Quatro momentos são entregues**: `PersonJoined`, `PersonLeft`,
`MessageReceived` e `MessageRemoved`. O `MessageEdited` saiu na 6: o produto não
tem verbo de editar mensagem, e um momento que nunca chega é pior que um que não
existe.

`carga` é JSON. Sempre. Faça `JSON.parse`.

**Três momentos a mais, só para o MOD que declara `maquina`** e só para **ele**
(nenhum outro MOD os recebe). São a forma de a metade de servidor saber o que a
pessoa dona da máquina respondeu, e o servidor sempre diz: um pedido não termina
em silêncio.

| momento | quando | `carga` |
|---|---|---|
| `MaquinaAceitou` | a pessoa viu a tela e aceitou, e a máquina subiu | `{ "servidor": "mc-1", "pessoa": 4 }` |
| `MaquinaRecusou` | a pessoa recusou, ou o pedido não pôde ser feito | `{ "servidor": "mc-1", "pessoa": 4, "motivo": "pelo-dono" }` |
| `MaquinaParou` | a máquina deixou de servir | `{ "servidor": "mc-1", "pessoa": 4, "motivo": "saiu" }` |

`servidor` é o nome do servidor de jogo que **você** deu no pedido, e `pessoa`, o
número de quem foi chamada. Os `motivo`s de `MaquinaRecusou` são `pelo-dono`,
`sem-tela` (o app daquela pessoa não tinha tela para mostrar a oferta),
`so-no-app` (quem recebeu o pedido era um navegador), `pacote-diferente` (o MOD
que ela tem não é o que o servidor ligou), `oferta-invalida` (a oferta não passou
na conferência dela) e três do servidor: `nao-esta-aqui` (a pessoa não está
conectada), `pendentes-demais` e `maquinas-demais` (os tetos do servidor). Os de
`MaquinaParou` são `pelo-dono`, `revogada`, `app-fechou` e `metade-falhou` (o que
a máquina informou), `saiu` (a pessoa saiu do servidor), `mod-desligado` e
`outro-pedido` (o servidor desligou o MOD, ou chamou outra máquina para o mesmo
jogo).

**Um momento que pede de novo a mesma coisa que acaba de ser recusada não gira.**
Se o seu `aoAcontecer` responde a um `MaquinaRecusou` com outro
`mundo.maquinas.pedir` à mesma pessoa, a mesma recusa do servidor, ou uma recusa
automática (`sem-tela`, `so-no-app`) que ela deu sozinha, dentro de **5 s**, não
volta como momento e não gera um novo pedido: fica no `seele.log`. O MOD já leu
essa recusa uma vez. Uma recusa que a pessoa dá (`pelo-dono`) é diferente: cada
pedido seu abre a tela de novo, e isso quem limita é ela.

Um MOD sem `aoAcontecer` não é erro: é um MOD cuja metade de servidor existe
para outra coisa.

### `dados` — o seu quintal

Um objeto de texto para texto. O que estiver nele ao fim da chamada é gravado; o
que estava lá chega preenchido.

```js
dados.placar = String(Number(dados.placar ?? "0") + 1);
```

- **Uma chamada é transacional.** Se o seu código lançar no meio, **nada** é
  gravado. Meia escrita é pior que nenhuma, porque você não teria como saber
  qual metade entrou.
- **Só texto.** Números viram texto; objetos, `JSON.stringify`.
- **Teto de 256 KiB**, e ele **recusa** em vez de aparar: acima dele, o pedido
  volta com `{ ok: false, error: "quintal-cheio" }`, e nada é gravado.
  `dados.uso()` diz quantos bytes o quintal ocupa (chave mais valor, em UTF-8,
  a conta da gravação), e `dados.teto` diz o teto.

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

### `mundo` — rede, relógio, registro, aviso e tema

```js
const resposta = mundo.buscar("https://api.exemplo.com/x");  // texto ou null
const agora = mundo.agora();                                  // segundos desde 1970
mundo.registrar("o que aconteceu");                           // vai para o log de quem hospeda
mundo.avisar({ canal: 3, carga: { revisao: 7 } });            // → { ok } ou { ok: false, erro }
mundo.tema({ fundo: "#0b1d2a", texto: "#f5f1e6" });           // → { ok } ou { ok: false, erro }
```

- **`http` e `https` e mais nada.** `file://` é recusado, senão a rede faria o
  que a pasta impede no disco.
- **Teto de 2 segundos por busca**, e ele existe por um motivo que vale você
  saber: o servidor roda um momento por vez, então uma busca lenta sua atrasa
  **todos os outros MODs** e a fila de eventos atrás deles. Dois segundos é o
  pior caso que alguém aceitou pagar por você. **Resposta acima de 4 MiB é
  descartada.**
- `mundo.registrar` sempre carrega o seu `id`. Você não escolhe o prefixo — uma
  linha cuja origem você pudesse forjar seria uma linha que ninguém pode usar
  para decidir qual MOD desligar.
- **`mundo.avisar({ canal?, carga })` avisa as janelas deste MOD**, e cada uma
  recebe o evento `servidor`, com a `carga` já lida. Vai a quem pode ler o
  canal (sem canal, ao servidor inteiro), inclusive à janela de quem pediu —
  que pode recebê-lo antes da resposta do próprio pedido. Só vale se a chamada
  terminar e o quintal gravar; dentro de uma chamada, vale o mais novo de cada
  canal. Até 1 KiB por aviso, 4 por segundo por MOD e 16 canais por chamada; o
  excedente fica, por canal, só o mais novo, e sai quando a janela abre. As
  recusas, como valor: `aviso-invalido`, `aviso-grande-demais`,
  `avisos-demais`. É o que deixa uma janela parar de perguntar de quatro em
  quatro segundos: `SeeleUI.capacidades()` traz `aviso-do-servidor`.
- **`mundo.tema(valores | null)` publica o tema do servidor**, só no MOD cujo
  manifesto declara `"tema": true`. Ele devolve `{ ok: true }` quando a forma é
  válida e o tema foi publicado — e não quando ele foi aplicado: o contraste é
  conferido por cada janela, que conta o desfecho pelo evento `tema`. As
  recusas vêm como valor, nunca como exceção (`sem-porta-de-tema`,
  `tema-de-outro-mod`, `tema-invalido`, `tema-grande-demais`,
  `chave-desconhecida`, `valor-invalido`, `regiao-desconhecida`), com a
  `chave`. O servidor guarda o tema, e ele sobrevive ao reinício; `null` o tira.
  O servidor não confere quem pediu: leia `contexto.admin`, se quiser restringir.

**`mundo.maquinas` existe só no MOD cujo manifesto declara `maquina`**; em
qualquer outro, `typeof mundo.maquinas` é `"undefined"`. É o par da porta de
quem joga: aqui a metade de servidor chama alguém a hospedar o jogo.

```js
mundo.maquinas.pedir({ quem: 4, servidor: "mc-1", oferta: { /* ver «A metade de máquina» */ } });
mundo.maquinas.avisar({ servidor: "mc-1", carga: { ligar: true } });
```

- **`pedir({ quem, servidor, oferta })`** chama a pessoa `quem` (o número, ou o
  texto de `contexto.person`) a rodar a máquina do servidor de jogo `servidor`
  (de 1 a 32 caracteres, `a-z`, `0-9` e `-`, sem `-` nas pontas). Não devolve
  nada: a resposta vem depois, como momento (`MaquinaAceitou`,
  `MaquinaRecusou`). É **declarativo, como o `mundo.avisar`**: o pedido só sai
  se a chamada terminar bem e o quintal gravar. Dentro de uma chamada vale o
  mais novo de cada `servidor`, e até 4 servidores de jogo.
- **`avisar({ servidor, carga })`** manda a `carga` (um valor JSON) à máquina
  aceita daquele `servidor` e **a ninguém mais**. Chega à metade de máquina por
  `maquina.aoAviso`, como texto. Até 1 KiB, 4 por segundo por MOD e 16 servidores
  por chamada; o excedente fica, por servidor, só o mais novo. **Isto é estado, e
  não um fluxo de comandos**: dois avisos seguidos ao mesmo servidor podem virar
  um só, o último. Sem máquina aceita, nada sai, e a chamada não lança: o
  `seele.log` diz `sem-maquina`.
- **O que lança**, com o nome na mensagem: `pedido-invalido` (o argumento não é
  um objeto), `servidor-invalido`, `quem-invalido`, `oferta-invalida:<campo>`
  (`<campo>` é o da regra que falhou, como `titulo`, `tuneis` ou
  `artefato_hash`, ou a **chave que a oferta não tem**: uma chave desconhecida é
  recusada, e não ignorada, para que `tunels` não vire uma oferta calada sem
  portas), `pedidos-de-maquina-demais`, `aviso-invalido`, `aviso-grande-demais`
  e `avisos-a-maquina-demais`. São exceções, ao contrário do `mundo.avisar`: um
  pedido errado deve falhar na chamada que o fez.
- **`aoPedir` ganha `contexto.maquina`**: `{ "servidor": "mc-1" }` quando quem
  pede é a **máquina aceita** daquele servidor de jogo (pelo `maquina.pedir`,
  adiante), e **ausente** em todo outro pedido. Nesse caso `contexto.person` é a
  pessoa que opera a máquina, e `contexto.channel`, `null`. Quem não é a máquina
  aceita não chega ao MOD: o servidor responde `sem-aceite` sozinho.

### `volume` — imagens grandes

Para a imagem que não cabe numa mensagem: até 10 MiB, sem passar pelo QuickJS.
Os bytes vão por um fluxo próprio, direto ao disco
([ADR 0048](adr/0048-mods-ganham-o-caminho-de-volume-que-os-anexos-ja-tem.md)).

```js
// dentro do aoPedir: autoriza um envio de quem pediu, por 30 segundos
volume.esperar(token, "retratos/coelho.png", ["png", "jpeg"], 30);  // → true/false
volume.tamanho("retratos/coelho.png");                              // → bytes ou null
volume.servir("retratos/coelho.png");                               // → o caminho servido, ou null
volume.apagar("retratos/coelho.png");                               // → true/false
```

- **`volume.esperar` só vale dentro do `aoPedir`.** A pessoa que pode enviar é a
  que está sendo atendida, e quem diz isso é o servidor, e não um argumento.
- **Os tipos são `png`, `jpeg`, `webp` e `gif`**, e o que chega é conferido
  pelos bytes, e não pelo nome. Um HEIC rejeita a janela com `tipo-recusado` e
  `detectado: 'heic'`.
- Do outro lado, a janela manda com `SeeleUI.enviar(arquivo, token)`, e a
  promessa resolve depois da gravação. **Ler de volta**: toda mídia de servidor
  aceita `doServidor: { canal, pedido: { …, transporte: 'volume' }, campo }`
  — a `midia`, o `fundoDeMidia`, o `retrato`, a `pessoa.capa` e o fundo de uma
  `tela` —, e o seu `aoPedir` responde
  `{ ok: true, volume: volume.servir(caminho) }`: o `servir` devolve o caminho
  quando o arquivo existe na sua pasta de volume, e `null` quando não. Sem `transporte: 'volume'`, a mídia vem em texto no `campo`, em
  páginas que o produto segue enquanto a resposta trouxer `proximo` (um objeto,
  somado ao pedido de partida).

### Os dois tetos que te cortam no servidor

| | quanto | o que acontece |
|---|---|---|
| memória | 8 MiB por MOD | a chamada morre; **o seu contexto sobrevive** |
| tempo | ~500 consultas ≈ 225 ms | a chamada morre; o seu contexto sobrevive |

Um laço infinito não derruba a sala: ele derruba a sua chamada. Mas um
`aoAcontecer` que falha **desliga** o MOD, e o desligamento é gravado — então um
bug que lança em todo evento tira o seu MOD do ar até alguém religá-lo.

Os 225 ms não são de relógio: são de trabalho. Uma máquina lenta te dá o mesmo
tanto de operações e demora mais, em vez de te cortar antes.

## A metade de máquina

Um servidor de jogo (um Minecraft, um Terraria) não roda dentro de um MOD, e o
MOD não pode ser quem decide, sozinho, que o computador de alguém passa a rodar
um programa e a abrir uma porta. Este é o desenho que cabe nessas duas frases:
**o MOD pede, a pessoa dona do computador vê o que vai acontecer e decide, e só
então a metade de máquina do MOD sobe nesse computador.** Quem joga abre a sua
ponta da porta com um clique.

São três metades trabalhando, e é bom saber qual faz o quê:

| metade | onde roda | o que faz |
|---|---|---|
| servidor (`servidor/main.js`) | a máquina de quem hospeda o servidor SEELE | chama alguém a hospedar o jogo (`mundo.maquinas.pedir`), ouve o aceite, avisa a máquina |
| máquina (`maquina/main.js`) | o computador de quem aceitou, **só no app** | abre a porta do jogo (`maquina.porta.servir`) e a fecha ao parar |
| cliente (`cliente/main.js`) | o computador de quem joga, **só no app** | mostra o botão; no clique, `SeeleUI.tunel.entrar(nome)` |

### O fluxo inteiro

**1. A metade de servidor pede.** Quem hospeda o servidor SEELE e administra o
jogo aperta um botão, e a metade de servidor decide quem é chamado (autorize pelo
`contexto`, como sempre):

```js
// servidor/main.js
globalThis.aoPedir = (contexto, pedido) => {
  const quem = JSON.parse(contexto);
  const o_que = JSON.parse(pedido);

  if (o_que.op === "hospedar") {
    if (!quem.admin) return JSON.stringify({ ok: false, erro: "so-admin" });
    mundo.maquinas.pedir({
      quem: quem.person,            // o texto de `contexto.person` serve
      servidor: "mc-1",
      oferta: {
        titulo: "Minecraft",
        receita: "minecraft-java",
        versao_da_receita: "1.0.0",
        versao_do_jogo: "1.21.4",
        tuneis: [{ nome: "mc", protocolo: "tcp", porta: 25565 }],
        artefatos: [{
          nome: "server.jar",
          url: "https://piston-data.mojang.com/server.jar",
          tamanho: 51000000,
          hash: { algoritmo: "sha256", hex: "…64 caracteres hexadecimais…" },
        }],
        origens: ["piston-data.mojang.com"],
        rodar: { comando: "java", argumentos: ["-jar", "server.jar", "nogui"] },
        disco_maximo: 2000000000,
        termos: [{ titulo: "EULA do Minecraft", url: "https://aka.ms/MinecraftEULA" }],
      },
    });
    return JSON.stringify({ ok: true });
  }

  // O pedido da própria máquina aceita (ver adiante): `contexto.maquina` existe.
  if (quem.maquina && o_que.pronto) {
    mundo.avisar({ carga: { servidor: quem.maquina.servidor, no_ar: true } });
    return JSON.stringify({ ok: true });
  }
  return JSON.stringify({ ok: false, erro: "operacao-desconhecida" });
};
```

Um campo ausente vira vazio (`""`, `[]`, `null` ou `0`), mas **nem todo campo
pode ficar vazio**: `titulo`, `receita`, `versao_da_receita` e `versao_do_jogo`
são textos não vazios; cada túnel tem `nome` válido, `porta` de 1 a 65535 e
`protocolo` `"tcp"` ou `"udp"` (outro valor, ou ausente, é `tuneis`); cada
artefato tem `tamanho` de pelo menos 1 e um `hash` válido (`sha1`, `sha256` ou
`sha512`, com o `hex` do tamanho dele). Uma oferta que não passa **lança na
hora**, com o nome da regra (`oferta-invalida:titulo`, `oferta-invalida:tuneis`,
`oferta-invalida:artefato_hash`), e não na máquina da outra pessoa.

**2. O dono vê a tela do produto.** O pedido chega ao app da pessoa chamada, e
**quem desenha a tela é o SEELE, e não o MOD**: o texto do MOD (o título do jogo,
os nomes, as URLs) entra nela como texto, nunca como forma, e nenhuma superfície
do MOD pode cobri-la ou imitá-la. A tela diz, nesta ordem:

- **quem pede, e em que servidor**, e o MOD, com o `id` dele;
- **o jogo**: o título e a versão, a receita e a versão dela;
- **a porta**: em qual porta o jogo escuta, e que quem está no servidor a alcança
  pelo SEELE;
- **o que vai baixar** (quantos arquivos, quanto no total, **de onde**) e **o que
  vai rodar** (o comando e os argumentos), ou que não há programa nenhum;
- **o disco**: até quanto ocupa, e em que pasta;
- **de onde o jogo baixa por conta própria**, sem o SEELE conferir, quando a
  oferta declara `dominios_do_jogo`;
- **os termos** do jogo: cada um com uma caixa para marcar, e a frase de que
  quem os aceita é a pessoa, e não o SEELE nem o MOD;
- o aviso da conta (ver «Avisar e isolar depois») e que ela **desliga quando
  quiser**, pela faixa no alto da tela.

**ACEITAR E LIGAR fica desligado até todas as caixas dos termos estarem
marcadas.** Esc é RECUSAR, e o clique fora da tela não responde nada: nada aceita
por acidente. Se a pessoa
já aceitou **exatamente isto** antes, o app não pergunta de novo; se algo mudou,
a tela diz o quê («isto mudou desde o seu aceite: o programa e os argumentos»).
Quem hospeda não vê a tela, e o MOD não a vê, não a pode ler e não a pode
calar: ele só fica sabendo o desfecho.

**3. `MaquinaAceitou`.** A metade de servidor recebe o momento (ou
`MaquinaRecusou`, com o motivo), e é aqui que se atualiza o que as janelas veem:

```js
globalThis.aoAcontecer = (momento, carga) => {
  const c = JSON.parse(carga);
  if (momento === "MaquinaAceitou") {
    mundo.avisar({ carga: { servidor: c.servidor, aceito: true } });
  } else if (momento === "MaquinaRecusou") {
    mundo.registrar(`${c.servidor}: recusado (${c.motivo})`);
  } else if (momento === "MaquinaParou") {
    mundo.avisar({ carga: { servidor: c.servidor, no_ar: false, motivo: c.motivo } });
  }
};
```

**4. A metade de máquina sobe e abre a porta.** Depois do aceite, o app lê o
`maquina/main.js` do pacote **que a pessoa aceitou** (o mesmo hash) e o roda num
contexto só dele. Aqui existe o objeto `maquina`:

```js
// maquina/main.js
const t = maquina.oferta.tuneis[0];           // a oferta que a pessoa aceitou, congelada

maquina.aoAviso((carga) => {                  // o `mundo.maquinas.avisar`
  const aviso = JSON.parse(carga);
  console.log(`aviso do servidor: ${carga}`);
});

maquina.aoParar(async (motivo) => {           // uma vez só; o app espera até 2 s
  await maquina.porta.parar(t.nome);
});

maquina.porta.servir({ nome: t.nome, protocolo: t.protocolo, porta: t.porta })
  .then(() => maquina.pedir({ pronto: true }))   // vai ao `aoPedir`, com `contexto.maquina`
  .catch((falha) => console.warn(`a porta não abriu: ${falha.erro}`));
```

- **`maquina.servidor`** é o nome do servidor de jogo. **`maquina.oferta`** é a
  oferta aceita, com os mesmos campos do pedido, congelada.
- **`maquina.porta.servir({ nome, protocolo, porta })`** abre a ponta do jogo:
  anuncia ao servidor que esta máquina serve o jogo `nome`. **Só serve o que a
  oferta aceita declara**: um nome, um protocolo ou uma porta que não estejam em
  `maquina.oferta.tuneis` são recusados com `fora-do-aceite`, antes de qualquer
  coisa ir ao fio. `maquina.porta.parar(nome)` a fecha, e
  `maquina.porta.quedas(nome)` devolve `[[motivo, quantas], …]`: por que as
  conexões de quem joga caíram. As três só existem com o poder `porta`.
- **`maquina.pedir(valor)`** manda `valor` (qualquer JSON) ao `aoPedir` da
  metade de servidor **como a máquina**: `contexto.maquina` vem como
  `{ servidor }`. Resolve com a resposta, em texto. É assim que a máquina diz
  «estou no ar». Rejeita com `sem-aceite`, `sem-resposta` ou `sessao-encerrada`.
- **`maquina.aoAviso(fn)`** recebe cada `mundo.maquinas.avisar` (o último ouvinte
  vence). **`maquina.aoParar(fn)`** é chamado uma vez, com o `motivo`, antes de a
  máquina cair; se devolver uma promessa, o app espera até **2 s** por ela.

**5. Quem joga clica.** Do outro lado, a metade de cliente da pessoa que quer
jogar desenha o botão, e **a porta se abre no clique**, e em nenhum outro lugar:

```js
// cliente/main.js
let pagina;
(async () => {
  pagina = await SeeleUI.superficies.criar({ id: "jogo", tipo: "pagina", titulo: "Jogo" });
  await pagina.montar([
    { forma: "botao", chave: "entrar", dentro: "ENTRAR NO JOGO" },
    { forma: "texto", chave: "t", dentro: "" },
  ]);
})();

SeeleUI.aoEvento(async (evento) => {
  if (evento.nome === "botao" && evento.chave === "entrar") {
    try {
      const porta = await SeeleUI.tunel.entrar("mc");   // a primeira linha do ouvinte
      await pagina.atualizar("t", { forma: "texto", chave: "t", dentro: `Jogo em 127.0.0.1:${porta}` });
    } catch (falha) {
      await pagina.atualizar("t", { forma: "texto", chave: "t", dentro: `Não abriu: ${falha.erro}` });
    }
  }
});
```

`SeeleUI.tunel.entrar(nome)` resolve com o número da porta em `127.0.0.1` onde o
jogo desta pessoa deve se conectar: a do próprio jogo, ou a primeira livre das 64
seguintes se aquela estiver ocupada. O produto desenha, fora de qualquer
superfície do MOD, uma faixa com cada porta aberta: a porta, o nome, o MOD, por
onde o tráfego anda, a latência e a última queda, em frase. **O MOD não a
desenha, não a cala e não a lê.** O clique que abre a porta é o consentimento
de quem clicou: por isso o `entrar` leva um **bilhete de gesto** que a casca emite
junto do evento `botao`, que vale 2 s e se gasta no primeiro uso. Chamá-lo sem
ele (num temporizador, num `then` de outra coisa, inventado, vencido ou já gasto)
rejeita com `fora-de-um-clique`, e nada vai ao núcleo. Chame-o na mesma volta do
ouvinte, **antes de qualquer `await`**.

**6. DESLIGAR, e `MaquinaParou`.** Enquanto a máquina está ligada, o app mostra
uma **faixa no alto de qualquer tela** (e a lista de aceites, em CONFIGURAÇÕES,
também tem DESLIGAR e REVOGAR), e ela diz o que está rodando. DESLIGAR chama o
`aoParar("pelo-dono")` da metade de máquina, fecha as portas, encerra a metade
e avisa o servidor, e a metade de servidor recebe `MaquinaParou` com o motivo. O
aceite continua guardado: o próximo pedido idêntico liga sozinho. REVOGAR faz o
mesmo e **apaga** o aceite, para que o próximo pergunte. A máquina também para,
sem a pessoa pedir, quando ela sai do servidor (`servidor-saiu`), fecha o app
(`app-fechou`), o MOD é desligado no servidor (`mod-desligado`), o servidor passa
o jogo a outra máquina (`outro-pedido`), o servidor deixa de reconhecer o aceite
(`sem-aceite`), ou a própria metade de máquina falha (`metade-falhou`; aí o
`aoParar` não é chamado, porque a metade já caiu). A tela da pessoa diz o motivo
em frase; o `MaquinaParou` que o MOD lê leva o nome da lista da seção
«aoAcontecer». **Os nomes não são os mesmos nas duas pontas**: o `aoParar(motivo)`
da metade de máquina recebe os do app (`servidor-saiu`, `sem-aceite`, …), e o
`MaquinaParou` da metade de servidor diz `saiu` para a mesma causa.

### O que o aceite prende

O que a pessoa viu e aceitou fica preso por um **resumo**: o SHA-256 de quatro
coisas, nesta ordem.

| entra no resumo | o que muda quando muda |
|---|---|
| o `id` do MOD | outro MOD pedindo o mesmo servidor de jogo não herda o aceite |
| o **hash do pacote** (o conteúdo do MOD, inclusive o `maquina/main.js`) | **uma versão nova do MOD pede o aceite de novo**, mesmo que a oferta seja igual: o código que roda na máquina é outro, e o aceite era do código que a pessoa autorizou |
| o nome do servidor de jogo (`servidor`) | outro `servidor` é outra máquina |
| a **oferta inteira**: `titulo`, `receita`, `versao_da_receita`, `versao_do_jogo`, `tuneis`, `artefatos` (nome, URL, tamanho, hash), `pack`, `origens`, `dominios_do_jogo`, `rodar` (comando e argumentos), `disco_maximo`, `termos` | qualquer um destes que mude faz o app perguntar de novo, e a tela diz **qual** |

**Não entra: quem pediu** (`pedido_por`). A mesma máquina, pedida por outra
pessoa do servidor, é o mesmo aceite. **Nem a configuração do jogo**: ela não é
campo da oferta, e o que o jogo lê para se configurar é arquivo da pasta dele,
que a pessoa que hospeda pode mudar sem pedir de novo ao dono da máquina.

**A tela e o resumo não são a mesma lista: o resumo prende mais do que a tela
mostra, e nunca menos.** A tela mostra o MOD, o jogo e a versão dele, a receita e
a versão dela, quem pediu e o servidor de jogo, cada porta com o protocolo,
**quantos** arquivos se baixam, o tamanho somado e de onde (as `origens`, ou os
hosts das URLs), o comando com os argumentos, os `dominios_do_jogo`, o teto de
disco com a pasta, e cada termo com a URL. Ela **não** mostra o `pack`, nem o
nome, a URL, o tamanho e o hash de cada arquivo um a um — e o resumo prende
todos eles. Tudo o que a tela mostra está no resumo, então nada do que a pessoa
viu muda depois do aceite sem invalidá-lo; e o que ela não viu um a um também
não muda calado: trocar o hash de um arquivo, ou o `pack`, pede o aceite de
novo, e a tela diz qual campo mudou (`o que se baixa`, `o pack`). O aceite
guarda o resumo, e não a oferta.

O `nome` de cada artefato é um nome de arquivo, e só: letras e dígitos ASCII,
`.`, `-` e `_`, até 256 bytes, e nem `.` nem `..`. Um nome com `/`, `\` ou
espaço faz a oferta inteira ser recusada (`oferta-invalida`): ele vira caminho
na pasta do servidor de jogo, e não pode sair dela.

### Quando o servidor SEELE reinicia

**O livro das máquinas do servidor mora na memória.** Os pedidos pendentes e os
aceites somem quando o servidor SEELE reinicia, e **nenhum `MaquinaParou` é
dado por eles**: não há quem o dê, porque o processo que lembrava deles acabou.
A metade de máquina, do lado de quem hospeda, para com `servidor-saiu` quando a
sessão cai. Então **a metade de servidor não pode supor que a máquina de antes
continua de pé depois de subir**: ao subir de novo, peça outra vez
(`mundo.maquinas.pedir`). Quem já aceitou a mesma oferta, com o mesmo pacote, é
ligado de novo sem tela.

E o pedido de novo, feito de um momento (`aoAcontecer`), pode não sair: por
5 s depois de uma recusa automática da mesma pessoa para o mesmo servidor de
jogo (`sem-tela` ou `so-no-app`), o servidor **descarta** o
pedido que um momento repete — é a trava contra o laço que o `MaquinaRecusou`
alimentaria sozinho. Ele não fica guardado para depois: fica no log, e o MOD não
ouve nada. Um pedido legítimo nesse intervalo (logo depois de a sessão de
alguém sair e voltar, por exemplo) precisa ser feito de novo depois dos 5 s,
num momento seguinte, e não num laço apertado. Um pedido vindo de um `aoPedir`
sempre sai.

### O que a metade de máquina não tem

Um contexto só dela, **sem tela e sem servidor**: não existem `SeeleUI`,
`SeeleMods`, `dados` nem `mundo` (nem `fetch`, nem `localStorage`). Ela fala por
três vias, e só: o `maquina`, o `console` (que escreve no `seele.log` de quem a
roda) e os relógios (`setTimeout`, `setInterval`). Quem quiser rede, estado ou
tela os pede à metade de servidor, por `maquina.pedir`; é a metade de servidor
que decide. Os mesmos tetos de memória e de tempo das outras metades valem.

### Os tetos e as recusas

| teto | quanto |
|---|---|
| nome do servidor de jogo | 32 caracteres |
| título | 64 bytes |
| texto livre (receita, versões, nome de arquivo, comando, argumento, título de termo) | 256 bytes |
| URL | 512 bytes, `https://` |
| portas na oferta | 8 (nome válido, porta de 1 a 65535, sem nome repetido) |
| arquivos a baixar | 16 (tamanho de 1 byte a 64 GiB, hash válido) |
| `origens` e `dominios_do_jogo` | 16 cada, só o nome do host (sem esquema, sem porta; `*.exemplo.com` vale) |
| argumentos do comando | 64 |
| termos | 4, cada um com título e URL `https` |
| `disco_maximo` | 64 GiB |
| pedidos por chamada | 4 servidores de jogo |
| avisos por chamada | 16 servidores de jogo |
| um aviso | 1 KiB, e 4 por segundo por MOD |
| pedidos pendentes e máquinas ligadas | tetos do servidor; acima, `MaquinaRecusou` com `pendentes-demais` ou `maquinas-demais` |

Os nomes que o MOD lê, e onde:

- **ao ligar o MOD (manifesto)**: `plataformas-pedem-api-nova`, `plataformas-vazias`,
  `plataforma-repetida`, `poderes-pedem-api-nova`, `poderes-vazios`,
  `poder-repetido`, `poderes-exigem-so-o-app`, `porta-sem-entrar-no-jogo`,
  `poderes-sem-maquina`, `maquina-sem-poderes`, `maquina-fora-do-pacote` e
  `maquina-sem-servidor`;
- **na chamada da metade de servidor** (lançados): os de `mundo.maquinas`, acima;
- **no momento `MaquinaRecusou`**: `pelo-dono`, `sem-tela`, `so-no-app`,
  `pacote-diferente`, `oferta-invalida`, `nao-esta-aqui`, `pendentes-demais`,
  `maquinas-demais`;
- **no momento `MaquinaParou`**: `pelo-dono`, `revogada`, `app-fechou`,
  `metade-falhou`, `saiu`, `mod-desligado`, `outro-pedido`;
- **na metade de máquina** (`maquina.pedir` e `maquina.porta.*` rejeitam com
  `e.erro`): `sem-aceite`, `sem-resposta`, `sessao-encerrada`, `fora-do-aceite`,
  e os do servidor, como `nome-tomado` (outra máquina já serve esse nome neste
  MOD) ou `tuneis-demais`;
- **na metade de cliente** (`SeeleUI.tunel.entrar` rejeita com `e.erro`):
  `fora-de-um-clique`, `so-no-app`, `nao-existe` (nenhuma máquina serve esse
  nome), `sem-permissao` (a pessoa não tem a ação `entrar-no-jogo`),
  `acao-nao-declarada`, `mod-sem-porta`, `nome-tomado`, `maquina-saiu`,
  `tuneis-demais`, `canais-esgotados`, `servidor-antigo` (o servidor é de uma
  versão que não leva jogo), `porta-local-ocupada`, `sem-resposta`, e
  `fora-do-aceite`.

### Avisar e isolar depois

Esta parte **avisa**; não isola. O servidor do jogo roda com a conta de quem
aceitou, e o que ele lê dentro desse computador não é separado do resto por
nada que o SEELE faça agora. Por isso a tela do aceite diz, em linha própria e
**fora da área que rola**, para que ninguém possa aceitar sem tê-la à vista:

> o servidor do jogo roda com a sua conta deste computador, e pode ler o que ela lê.

Escreva o MOD sabendo disso. O que a oferta declara que vai baixar e rodar fica
preso ao aceite desde já (o `rodar`, os `artefatos`, os `dominios_do_jogo`), mas
**quem baixa e executa o programa é o poder `programa`**, reservado à parte
seguinte: nesta, o aceite, a metade de máquina e a porta existem, e o programa
não. Não prometa a quem hospeda o que o produto ainda não faz, e não peça mais
do que o jogo precisa: uma oferta com `disco_maximo` e `origens` justos é uma
oferta que se aceita.

## No celular e no navegador

A mesma API, o mesmo motor e os mesmos limites. O que muda é a casca, e o MOD
sabe dela por `retrato.onde`, `retrato.regioes` e o evento `lugar`.

- **No celular, toda região diz se existe.** Sete das oito existem; o
  `status.barra` não. Uma contribuição para ele é aceita e recebe
  `regiao-ausente`.
- **«Tela cheia» é a tela acima da faixa de voz e das abas**: nenhuma superfície
  as cobre nem as torna inertes. O diálogo modal ocupa a sessão acima do pé, com
  as `acoes` fixas no rodapé do produto; o não modal fica embaixo, até 85% da
  altura dela; a folha é uma folha de baixo, que cresce com o conteúdo.
- **No toque, um dedo sobre uma figura com `chave` arrasta** (o `traco`, com
  `cancelou` quando o gesto não termina); um dedo no vazio de uma `tela` rola a
  página. Pinça, área de acerto mínima e toque longo não estão na 6.
- **No navegador, `SeeleUI.enviar` e o seletor de arquivo não existem**, como o
  que a plataforma não tem não existe.
- **`plataformas` (API 8) diz onde o MOD roda**, e o celular conta como
  `"navegador"`. Declare só `"app"` se o MOD depende do que só o app tem; o
  celular e o navegador o pulam. **Um limite, dito como é**: antes de entrar, o
  anúncio de aceite que o servidor manda não leva as plataformas (mensagem do
  fio, ADR 0058), então um navegador pode baixar o pacote de um MOD só do app
  durante o aceite. Ele **nunca o executa**; as plataformas chegam no catálogo
  depois da entrada, e é aí que a casca o pula. Levá-las antes da entrada fica
  para o plano seguinte. **`"plataformas": ["app"]` é também o que um MOD com
  `poderes` tem de declarar**: o túnel e a metade de máquina abrem socket e
  rodam coisa no computador, e o navegador (o celular com ele) não faz nenhum
  dos dois; um MOD assim pula nele, e `SeeleUI.tunel.entrar` rejeita com
  `so-no-app` se algum dia o chamarem lá. Para o que os MODs oficiais fazem no navegador hoje, ver
  [`docs/evidencias/api-8/mods-oficiais-no-navegador.md`](evidencias/api-8/mods-oficiais-no-navegador.md).

## Testar antes de publicar

Em CONFIGURAÇÕES › MODS › INSTALADOS, **INSTALAR MOD DE UMA PASTA** aponta para
a pasta do seu MOD. O `mod.json` é lido antes de qualquer byte ser copiado, e o
que não serve é recusado pelo nome — `api-too-old`, `malformed-id`, `empty`,
`tema-sem-metade-de-servidor` e o resto. Instalar não liga: depois, hospede e
habilite.

O pacote instalado mora numa pasta com o hash do conteúdo
(`mod-packages/<hash>/`, na pasta de configuração), e por isso **editar os
arquivos lá dentro não funciona**: um pacote mexido depois de instalado é
recusado, e nomeado. Mudou o MOD, instale de novo.

**A metade de servidor, sem hospedar pelo app.** Com este repositório, um
servidor local sobe com o seu MOD já ligado:

```sh
cargo run -p seele-server --example servidor-com-mod -- /caminho/absoluto/do/mod /caminho/absoluto/de/um/mundo-novo
```

O exemplo instala o pacote pelo hash, no mesmo lugar em que o servidor o
procura, liga o MOD antes de subir (é a subida que entrega os momentos do
`aoAcontecer`), e escuta em `127.0.0.1`, na porta 8390. Uma terceira palavra
escolhe outra porta, menos a 8384, que é a da bateria de testes do repositório.
Ele imprime o endereço e a impressão digital, e o app entra por eles: é assim
que a metade de janela aparece. O que o MOD não conseguiu carregar sai no
terminal. **O que ele não faz:** não abre janela, não acompanha as suas edições
(mudou o MOD, rode de novo num mundo novo; a pasta do mundo não pode existir,
para um mundo nunca ser reaberto com outro código) e não fala com o indexador.

**Erros do seu MOD aparecem no `seele.log`**
(`~/.config/seele/seele.log`), com o seu `id` num campo próprio, `mod_id=`: os
da metade de servidor no de quem hospeda, e os da metade de janela no de cada
máquina que roda o MOD. A seção seguinte diz como lê-los.

> **O que falta, e é honesto dizer:** não há `seele mod new`, não há definições
> de tipo para o seu editor, e a metade de janela só roda dentro do app.
> O ADR 0045 promete as definições de tipo e elas ainda não existem. Enquanto
> não existirem, o retorno que você tem é o log e a aba DIAGNÓSTICO.

## Quando o MOD não faz o que devia

### O `console`, e onde ele escreve

A metade de janela tem `console`, e cada chamada vira **uma linha no
`seele.log` da máquina que roda o MOD**, no nível que você escolheu. A metade
de servidor não tem: lá, `console.log(…)` lança `ReferenceError`, e um
`aoAcontecer` que lança desliga o MOD. O registro dela é o `mundo.registrar`,
que vai ao log de quem hospeda com o mesmo `mod_id=`.

Quando a metade de servidor lança, a linha de quem hospeda traz o que ela
lançou: o texto, entre aspas e escapado, e, quando o lançado é um `Error`, a
primeira linha da pilha, que diz onde —
`mod threw: "ReferenceError: console is not defined" at "<anonymous> (eval_script:1:29)"`.
Um `throw` no topo do arquivo, ou um erro de sintaxe, sai como
`mod threw while loading: …`. Quando nada foi lançado, mas o motor recusou o
que o MOD lhe deu — um `aoPedir` que falta, um que devolve outra coisa que
texto, um valor que não é texto em `dados` —, a linha diz `mod failed: …`, com
o texto do motor: `mod failed: "Error converting from js 'int' into type 'string'"`.
O texto e o onde, juntos, são cortados em 512 caracteres contados já escapados.

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
a linha seguinte diz quantas foram seguradas (`suprimidas=`). Se o seu código
se calar depois da rajada, o próprio produto escreve essa linha, em WARN, em até
meio segundo, sem esperar o MOD parar. Enquanto o MOD está de pé, ele só a
escreve com uma vaga de sobra no teto, deixando a seguinte para o seu código:
quem tenta escrever ao menos uma linha a cada quarto de segundo leva a conta
na própria linha, e o produto não fala. Quem escreve mais espaçado — várias
linhas de uma vez, a cada meio segundo, por exemplo — pode ver a linha do
produto entre as suas, ocupando uma vaga que sobrou. Quando o MOD para, a
conta que restou sai de qualquer jeito, como a última linha dele.

### Ler a linha

Cada linha do `console` sai assim:

```text
… WARN seele_app: console do MOD mod_id=fulano/meu-mod texto="o retrato não veio"
```

O texto vem entre aspas e escapado: uma quebra de linha nele não vira outra
linha do registro. Para ver só o seu MOD — o `console`, as mídias que a janela
recusou, as montagens que não couberam e o que o servidor disse dele, que usam o
mesmo campo:

```sh
grep "mod_id=fulano/meu-mod" ~/.config/seele/seele.log
```

O arquivo fica em `~/.config/seele/` no macOS e no Linux (ou em `$SEELE_HOME`,
quando definido) e em `%APPDATA%\tech.datadev.seele\` no Windows. Para gravar
também o `console.debug`, abra o app com `RUST_LOG=info,seele_app=debug` no
ambiente.

### A aba DIAGNÓSTICO

Em CONFIGURAÇÕES › MODS › DIAGNÓSTICO, QUEM PINTA CADA LUGAR: os pontos onde um
MOD pode entrar nesta versão, sempre todos, e não só os que alguém disputa. Para
cada um, quem desenha agora (o SEELE, um MOD, ou um para cada pessoa que
declarou), quem pediu e perdeu, a escolha desta máquina, e a mídia em uso —
quantas carregando, prontas e recusadas, com o motivo da recusa. As
contribuições que não têm onde entrar (`regiao-ausente`) também são contadas
ali. «Nenhum MOD usa este lugar agora» é a resposta que você mais precisa ler
quando o seu não aparece. É só leitura: escolher quem desenha continua em
INSTALADOS, «QUEM DESENHA O QUE».

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
- **O `api` que você declara é uma versão só.** O app 0.16 executa a 6; um MOD
  das APIs 3 a 5 continua rodando na linha do Servidor 0.15, com o que a versão
  dele tinha. Para a 6, publique de novo — e passe pela avaliação de novo.
- **Uma versão publicada da API nunca é editada.** `api/congeladas.sha256` e o
  teste `a_api_publicada_nao_se_edita` reprovam o build quando os bytes de uma
  `vN.json` mudam. Isso não é o mesmo que a API nunca perder nada: a 6 tirou a
  região, os cartões e o `snapshot`, e por isso um MOD da 5 não roda no 0.16
  ([`api/README.md`](../api/README.md), «A API 6»). Renomear por dentro
  continua sendo problema nosso, e há um build vermelho que cobra isso de nós,
  não de você: o `cargo xtask check-api`.
