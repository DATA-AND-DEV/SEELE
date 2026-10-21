# A auditoria do `api/v6.json` contra o produto

Pedida pela orquestradora em 15/10, depois que o porte da MESA achou o
`cancelou` do `traco`: prometido no `v6.json` e não entregue (consertado no
`c41d04eb`). O congelamento conferiu o **texto** de cada promessa, e não a
**entrega**. Esta auditoria confere as duas coisas, entrada por entrada:

- **o código que entrega a promessa**;
- **a prova**: uma bancada ou um teste que reprovaria se a entrega sumisse, e
  que roda em algum lugar.

Levantada em `48b6a1ba` (ramo `0.16/api-6-corte`) por nove frentes de leitura,
uma tabela por grupo de seções. As frentes não rodaram `cargo` nem mexeram no
repositório. As reversões que as tabelas dão como «medido» rodaram numa cópia,
fora do repositório, e só com as bancadas de Node. Os números de linha valem
para `48b6a1ba`.

## As tabelas

| Arquivo | Seções do `v6.json` |
|---|---|
| [1-compat-reads.md](1-compat-reads.md) | `version`, `notes`, `compatibilidade`, `handlers`, `reads`, `retrato-fica-de-fora` |
| [2-actions-world.md](2-actions-world.md) | `actions`, `own`, `world`, `sentido` |
| [3-eventos.md](3-eventos.md) | `moments`, `eventos`, `eventos-mudam`, `eventos-da-tela` |
| [4-formas-figuras.md](4-formas-figuras.md) | `formas`, `figuras` |
| [5-regioes.md](5-regioes.md) | `regioes` |
| [6-contribuicoes-capa.md](6-contribuicoes-capa.md) | `contribuicoes`, `capa` |
| [7-superficies-camadas.md](7-superficies-camadas.md) | `superficies`, `camadas` |
| [8-estilos-tema.md](8-estilos-tema.md) | `estilos`, `tema` |
| [9-nucleo-som-recusas.md](9-nucleo-som-recusas.md) | `nucleo-de-confianca`, `som`, `volume`, `limites`, `celular`, `recusas` |

## Os estados

- **PROVADA**: o código entrega, e um teste que roda reprovaria sem a entrega.
- **INCERTA**: o código entrega, mas não é óbvio que o teste reprova sem ela.
  Muitas destas já foram revertidas pelas frentes **e passaram**, o que as faz,
  na prática, SEM PROVA.
- **SEM PROVA**: o código entrega, e nenhum teste que roda a cobre.
- **SEM CÓDIGO**: nada entrega a promessa, ou o código contradiz o `v6.json`.
- **NÃO SE APLICA**: texto sem promessa verificável.

## A contagem, em 48b6a1ba

Uma entrada composta conta uma vez por parte com estado próprio, e por isso o
total passa do número de chaves.

| | PROVADA | INCERTA | SEM PROVA | SEM CÓDIGO | NÃO SE APLICA | linhas |
|---|---:|---:|---:|---:|---:|---:|
| 1 compat e leituras | 40 | 9 | 0 | 1 | 2 | 52 |
| 2 ações e mundo | 38 | 13 | 5 | 1 | 0 | 57 |
| 3 eventos | 14 | 23 | 0 | 3 | 0 | 40 |
| 4 formas e figuras | 18 | 0 | 16 | 0 | 0 | 34 |
| 5 regiões | 32 | 11 | 13 | 3 | 0 | 59 |
| 6 contribuições e capa | 31 | 15 | 5 | 4 | 3 | 58 |
| 7 superfícies e camadas | 36 | 10 | 11 | 6 | 2 | 65 |
| 8 estilos e tema | 48 | 12 | 27 | 7 | 4 | 98 |
| 9 núcleo, som, volume, recusas | 49 | 14 | 4 | 9 | 5 | 81 |
| **total** | **306** | **107** | **81** | **34** | **16** | **544** |

Consertadas até aqui: **1**, o `cancelou` do `traco` (`c41d04eb`), antes da
auditoria, e já contada como PROVADA. Esta contagem é o ponto de partida. Os
consertos e os achados entram na seção de baixo, com os commits.

## Os achados que valem para muitas linhas

1. **As bancadas de navegador não estão no portão de release.** O `ci.yml`
   dispara só por `workflow_dispatch`, e a bateria do `publicar.sh` roda as
   bancadas de Node (`check-runtime`), os roteiros, o `.wasm`, o `check-api`,
   os testes e o clippy, mas nenhuma das 22 bancadas de navegador do job
   `bancadas`. Muitas PROVADA dependem só delas, entre elas `camadas`, `capa`,
   `nucleo`, `regioes-do-produto` e `saidas-de-quem-hospeda`.
2. **A `tools/bancada-do-celular.py` não roda em lugar nenhum**: nem `ci.yml`,
   nem `release.yml`, nem `publicar.sh`, nem `xtask`. O guarda
   `toda_bancada_da_pasta_roda_em_algum_lugar` não a vê, porque ela mora em
   `tools/`. Toda promessa do celular que só ela mede conta como SEM PROVA.
3. **O `check-api` confere texto, e não a entrega:**
   - o braço de `momento_de`, e não quem emite o momento (o `MessageEdited` não
     tem quem o emita);
   - nenhum espelho entre `formas`/`figuras` do `v6.json` e o renderer, embora
     o rascunho, no §13, diga que haveria;
   - os nomes de `reads`, mas não o que o servidor entrega neles: `reads` diz
     `contexto.pessoa` e `contexto.canal`, e o servidor entrega `person` e
     `channel`, desde a v2.
4. **O tipo de mensagem do prelúdio e o `case` do roteador não estão
   amarrados.** Só 5 dos 20 tipos são conferidos rodando o prelúdio, e trocar
   a string de um lado só não reprova nos outros 15.
5. **Uma forma desconhecida some calada** nas regiões e nas superfícies: a
   recusa só é contada no cartão, e o `superficie-montar` responde `ok`. É o
   «o produto sabe e não conta».

## As 34 SEM CÓDIGO, separadas para decisão

A triagem é minha. Cada item será conferido no código antes do conserto, como
foi o das recusas do volume.

### A. Errata do texto: o código está certo, ou decidido, e o `v6.json` não diz isso

Mexer no texto é descongelar o `v6.json`, e a decisão é da orquestradora.

- `sentido.superficie-atualizar`: «as APIs 4 e 5 também o veem» é falso desde
  o corte.
- `regioes.servidor.navegacao.pontos[1]`: o `sala.acoes` mora na lista de
  pessoas, e não na navegação, desde `26ad0c2b`.
- `regioes.redes.celular.forma`: os itens têm 44 × 44, e não 56 × 44.
- `contribuicoes.servidor.aparencia`: «o MOD de janela pinta por ele como na
  5, e o tema do servidor se soma». Na 0.16 a janela só faz prévia, e o ponto
  veta o tema de outro MOD.
- `capa.movimento-reduzido`: não existe preferência própria do SEELE, só a do
  sistema.
- `superficies.desconhecido` (b): «até a 5 abre como dialogo». A 5 é recusada
  antes.
- `tema.derivados.nomes`: o `bordaForte` está na lista dos derivados, e a
  `formula` diz que não é derivado.
- `tema.conferencia`: «na recusa de `mundo.tema`». O servidor não confere
  contraste, por decisão de 10/10.
- `nucleo-de-confianca.regra`: o apelido (por desenho, rascunho §10) e a
  atribuição são alcançados pelo tema.
- `reads.contexto`: `pessoa` e `canal` contra `person` e `channel`.

### B. Entregas que cabem na API 6, e que eu conserto com o vermelho antes

Estão na ordem do caminho dos portes.

1. **As recusas do volume**: `sem-espera`, `nao-gravei` e `incompleto` chegam
   ao MOD como `"imagem recusada: SemEspera"` (`volume.rs:602`, o nome de
   `Debug` do enum). A bancada passa porque o dublê já rejeita com o nome
   pronto. Conferido no código em 15/10. E o `tipo-recusado` só existe no
   seletor, e não no volume.
2. **`recusas.o-que-e`, a lista fechada.** O tipo desconhecido recebe uma
   frase, e `too-many-requests`, `fila-cheia` e `mensagem-grande…` saem sem
   `e.erro`. Oito nomes que o `v6.json` não lista chegam ao MOD.
3. **O K2**: o retrato que não decodifica não manda `midia`/`falhou`, e a
   `regiao-do-mod.cjs` exige o contrário do `v6.json`.
4. **`formularios.erro`**: `deslizante`, `marca` e `interruptor` não mostram
   o `erro`, ficam fora do `sujo` e são recusados em `formulario.erros`.
5. **`tema.chaves`, «recusados pelo nome, na casca»**: a prévia devolve
   `{erro:'falhou'}` para chave fora da tabela, valor fora da forma e `modo`
   que discorda.
6. **`estilos.estados` `escolhido`**: nada escreve `data-escolhido`.
7. **`estilos.celular`, os 44 px**: o estilo do MOD, inline, vence o piso da
   folha. Medido: 20×20, e 9×9 com `escalar: 0.2`.
8. **A capa «parada» na lista**, e a animação que segue com a janela oculta
   (nada lê `document.hidden`).
9. **`eventos-da-tela.lugar`, «a região mudou de tamanho»**: só o
   `chamada.palco` o manda.
10. **`compatibilidade.por-plataforma`**: nada tira o `enviar` e o `arquivo`
    no navegador.
11. **A forma desconhecida contada** (achado 5), nas regiões e nas superfícies.

### C. O que eu não vejo como entregar na 0.16: decisão da orquestradora

- `moments` `MessageEdited`: o produto não tem verbo de editar mensagem.
- `tema.porta`, «na tela de aceite»: o `ModAnunciado` e o `ModExigido` não
  levam `tema`. É mudança de fio, e é da F1.
- `superficies.folha.celular`, «expansível»: não há estado nem gesto.
- `superficies.chamada.palco.celular` (b): não existe a tira de quem fala no
  alto.
- `nucleo-de-confianca.celular[2]`, «DE ONDE?»: não há `#origem` no HEAD. Veio
  de `86326686`, que não é ancestral.
- `celular.toque-na-tela`: só o arraste existe. Não há pinça, área mínima nem
  toque longo, e o `touch-action: none` impede rolar no vazio.
- `superficies.limites-o-que-e` (b), «10 MiB por mídia»: o teto é a soma que a
  `midia` divide com o `retrato` e o `fundoDeMidia`. É o Step 3 da tarefa 7,
  que está em aberto.
- `superficies.garantias[5]`, a decisão de descartar e o `descartou`: só existe
  com `fecharComAlteracoes: 'confirmar'`, que nenhum `vN.json` declara. Por
  padrão, a superfície suja fecha sem perguntar. Ou o padrão muda, ou o texto.
- `volume.reducao`: só a `pessoa.capa` é reduzida, e ela conta bytes
  decodificados.
- `tema.formula`, o guia: nenhum guia publica `tonsDoTema`, e
  `docs/como-se-faz-um-mod.md` ainda ensina o tema da API 5.

## Consertos e achados

As decisões da orquestradora estão na D-api-8 (`b07c6d64`, na `versao/0.16`).
Cada conserto entrou com o vermelho antes e com as reversões, que estão na
mensagem do commit.

**A emenda única do `v6.json`:** `44025ab5`. Ela trouxe as dez erratas (A), o
que sai (C1, C3, C4, a pinça e o «DE ONDE?»), o teto real das mídias, a
redução e a lista fechada das recusas inteira. Com ela entrou o guarda
`a_lista_fechada_das_recusas_e_a_do_v6`.

**Os consertos da API 6 (B):**

| | entrega | commits |
|---|---|---|
| B1 | as recusas do volume pelo nome | `472ed664`, `f52a84ec` |
| B2 | a lista fechada na fronteira da janela, o prelúdio, o tipo desconhecido | `0c2b3dc7`, `0908eaa5` |
| — | a `forma-desconhecida`, recusada no montar e no atualizar | `1c9006e3` |
| B3 | o retrato que não carrega manda `midia`/`falhou` (K2) | `ba8afdf7` |
| B4 | o `erro`, o `sujo` e o `formulario.erros` no deslizante, na marca e no interruptor | `fb98b98d` |
| B5 | a prévia do tema recusa pelo nome | `854a7afc` |
| B6 | o estado `escolhido` | `2f76bc4c` |
| B7 | o piso de 44 px no celular | `c64f29c6` |
| B8 | a capa parada com a janela oculta e na lista | `ad4c5e71`, `821ac8df` |
| B9 | o `lugar` das regiões | `43d0b92e` |
| B10 | as capacidades por plataforma no navegador | `3f9e62d0` |
| C6 | no toque, só a figura que se arrasta prende o dedo; o vazio rola | `821877a9` |

**Os achados dos portes durante a auditoria**, também consertados:

| achado | quem achou | commit |
|---|---|---|
| o `cancelou` do `traco` | MESA | `c41d04eb` |
| a citação `daPessoa` que não acompanhava a troca | PERFIS | `fc7f32d2` |
| a capa parada na lista | PERFIS | `821ac8df` |
| a superfície suja que só perguntava com uma opção (`garantias[5]`) | auditoria, C | `0e9f8fbf` |
| a citação que não resolvia numa superfície (`capa.onde.perfil`, D-api-9) | PERFIS | `1f701ebc` |
| o `inicial` «da montagem», que se lia «de cada montagem» | MESA | `563ecf8f` (texto) |
| o par do segundo apagado com outra grafia (`apagado-secundario`) | ESTILO | `0938697d` |
| as cores de estado do par sem nome no contrato | ESTILO | `563ecf8f` (texto) |
| o diálogo no celular descrito como tela cheia para os dois, e as abas do ESTILO | F8 | `563ecf8f` (texto) |

**A segunda emenda:** `563ecf8f` (SHA-256 da v6 `58576e95…`). Ela escreveu a
`pessoa` da citação, a errata do `sujo`, o diálogo no celular, os nomes do par
de uma recusa do tema e o `teto` do `teto-de-alvos`.

**A terceira emenda:** `5b7ff153` (SHA-256 da v6 `ddbd76f4…`), com a entrega
no mesmo commit, porque o guarda da lista fechada não deixa um nome novo de
recusa entrar sem o texto. As duas decisões da orquestradora:
- a montagem que não coube inteira recusa com `nos-recusados`, a conta e a
  amostra (até oito), no montar, no atualizar e nas classes; antes respondia
  `falhou`;
- o `classes` numa contribuição é contado nos `recusados` do evento
  `contribuicao`, com a amostra (`classes-fora-da-superficie`).

Os três portes a conferiram: o PERFIS sem remontar (83/83), o ESTILO no
pacote `6552cf6`, e a MESA com a frase da recusa e um defeito dela achado e
consertado (as classes da página declaradas antes de a dar por aberta).
Daqui em diante, o que pedir texto vai numa quarta e última emenda, no fim do
passo 7 (a orquestradora, 16/10).

## O passo 5: o que os portes usam

Três frentes de leitura levantaram, nos ramos dos portes (a MESA em
`porte/api-6`, o PERFIS em `porte/4.0.0`, o ESTILO em `porte/api-6`), o que
cada MOD usa da API 6 e de que comportamento do produto depende sem chamar.
Uma quarta cruzou as 25 dependências com o código e as provas, em `0e9f8fbf`.

**O que o produto não fazia como o MOD esperava**, consertado:

| dependência | conserto |
|---|---|
| o `teto-de-alvos` com `teto` (o PERFIS lê `erro.teto`) | `477e6ff0` |
| a parte errada de uma propriedade composta do estilo, que virava o padrão calada (a cor «vermelha» de uma sombra saía preta) | `6caf0b2d` |

**As provas novas**, cada uma com as reversões na mensagem do commit:

| o que | onde | commit |
|---|---|---|
| as 30 formas e as 4 figuras do `v6.json`, com o espelho nos dois sentidos | `regiao-do-mod.cjs` (portão) | `09c9a375` |
| a citação com `pessoa` | `regiao-do-mod.cjs` (portão), `capa.cjs` | `1f701ebc`, `c9749e03` |
| fechada, a superfície continua do MOD; `criar` com o mesmo id a reabre | `camadas.cjs` | `e43e4520` |
| `atualizar` uma contribuição que saiu devolve `false` | `contribuicoes-e-camadas.cjs` (portão) | `4dd1918a` |
| as frases da validação, o `minimo` sozinho, a marca booleana e a escolha em texto no `submeter` | `camadas.cjs` | `d3d04527` |
| as consultas pela largura da superfície | `camadas.cjs` | `71c976a7` |
| o `saiu` do evento `pessoas` | `eventos-da-tela.cjs` (portão) | `b3a46064` |
| o clique numa ação de contribuição com o alvo: `pessoa` no cartão, `canal` na sala | `capa.cjs` | `7e122469` |
| a saída da sessão solta a superfície suja sem perguntar | `camadas.cjs` | `34e4703c` |
| o `ModAviso` com os nomes que o Rust serializa | `frontend.rs` (portão) | `46e57cf6` |
| a errata do `inicial`: o da primeira montagem do campo | `camadas.cjs` | `d4a9dc51` |
| o `tonsDoTema` do guia faz o que a casca faz | `tons-do-tema.cjs` (portão) | `c47b6874` |
| o diálogo não modal e o rodapé do modal no celular, pela F8 | `tools/bancada-do-celular.py` | `cf031786` (na `0.16/celular`) |

O `c9749e03` conserta uma conferência minha que não media nada: o caso 5c da
`capa.cjs` contava os eventos por `m.evento.nome`, que não existe.

**O que fica de fora, e por quê:**
- o `lugar` da sessão sai só quando a casca muda de forma, como o texto diz, e
  não ao chegar nem quando a janela muda de tamanho; vai para o guia;
- o evento `acao` traz `pessoa` e `canal` em texto, e `atualizar` de quem saiu
  do servidor devolve `true` (o produto não revoga por presença); os dois vão
  para o guia.

**O espelho do prelúdio com o roteador** (o achado 4): `154537e7`. Os tipos
das duas pontas são o mesmo conjunto, e cada um é montado pela capacidade que o
roteador confere. O espelho achou o `pedaco` e o `soltar` montados sem a
`arquivo`, que no navegador existiam e sempre falhavam. As formas e as figuras
têm o espelho em `09c9a375`, em runtime e no portão.

**O guia:** `c47b6874`, na API 6, com o `tonsDoTema` executado pela bateria.

**A linha do tema na tela de aceite:** `abea7352`, com a `aceite-dos-mods.cjs`
no portão.

**As limpezas das APIs 3 a 5**, sobre a integração da versao/0.16
(`29f6454f`): o prelúdio sem `snapshot`, `regiao` e `cartoes` e o roteador
sem os casos deles (`282c8e7c`); a lista de pessoas sem os cartões da 3
(`a84f7da4`, o `tela-sessao.js` isolado); as provas do cartão no perfil da 6
(`9ab16437`); a região e os cartões fora da casca, com os apontadores das
congeladas que o corte aposentou ditos pelo `check-api` (`960290af`); e as
regras de folha que ninguém casa mais (`8f589060`). Dois defeitos da
integração apareceram na casca inteira e foram consertados à parte: a
bancada das saídas de quem hospeda, que o `ci.yml` rodava e o `publicar.sh`
não (`aa306a5d`), e a prova do núcleo que media um ponto escondido atrás da
barra quando a faixa de quem liga empurra a tela (`bc8bd1a5`).

## O passo 7: as INCERTA e as SEM PROVA restantes

A regra da orquestradora (16/10): consertar o produto sem mexer no texto do
`v6.json` sempre que der, e o que pedir texto vai numa quarta e última
emenda, no fim. Cada prova foi revertida contra o produto e reprovou; as
reversões estão na mensagem de cada commit. As linhas estão remarcadas nas
tabelas, com a prova e o commit.

| lote | o que | onde | commit |
|---|---|---|---|
| 1 | as nove categorias, os sete estados, as cinco animações e o `fora` dos estilos, com o espelho | `regiao-do-mod.cjs` | `c7d19051` |
| 2 | os tetos da superfície contra o texto; **a fundura cortava calada**, consertada (`teto-de-fundura`) | `regiao-do-mod.cjs` | `695dbf18` |
| 3 | os eventos `escolha`, `numero`, `cor` e `arquivo` | `regiao-do-mod.cjs` | `c19cd321` |
| 4 | o fim que diz só `terminou`, o laço pausado dentro da volta, o teto de figuras | `regiao-do-mod.cjs` | `10439d06` |
| 5 | o `lugar` da sessão, nada da ligação, a cola do desenho e o `listen` do aviso | `eventos-da-tela.cjs` | `54eaf66f` |
| 6 | o título, o ocultar e o `fechar-pedido` pelo roteador; os `cartoes` recusados na 6 | `camadas.cjs`, `contribuicoes-e-camadas.cjs` | `f5ee4da1` |
| 7 | o link pelo dono de verdade; a forma e a preferência da capa; o ponto desconhecido na linha | `capa.cjs`, `contribuicoes-e-camadas.cjs` | `6946541f` |
| 8 | cada par do tema recusa por ele, com o espelho | `tons-do-tema.cjs` | `4d68c2a9` |
| 9 | o ouvinte do `aoEvento` recebe o evento | `casos.json` (nativo e `.wasm`) | `61ba268f` |
| 10 | a recusa da API velha e a linha do cache apontam o launcher | `hospedar-com-mod-antigo.cjs` | `fe0a72cb` |
| 11 | o contexto diz quem pediu e se ela administra | `pedidos.rs` | `41049fc7` |
| 12 | o teto de canais do `mundo.avisar` e o `avisos-demais` | `aviso.rs` | `c8914ed0` |
| 13 | o `TemaMudou` só do pedido que gravou; qualquer pessoa pede | `tema_do_servidor.rs` | `d69fe061` |
| 14 | listar e apagar pelo JavaScript, o `volume.apagar`, a linha do `mundo.registrar` | `mods/mod.rs` | `4c825327` |
| 15 | cada momento sai do evento dele, com a carga | `despacho.rs` | `36fc80bd` |
| 16 | o pedaço só para quem escolheu, o `soltar`, o teto do escolhido, a mídia acima do teto | `main.rs` | `7fd1246c` |
| 17 | uma imagem de MOD vai e volta pelo cliente de verdade | `ponte_de_pedidos.rs` | `a4a2b7a4` |
| 18 | o palco da chamada monta com o perfil da superfície, do texto à montagem | `contribuicoes-e-camadas.cjs` | `dc7ada28` |
| 19 | o arredondamento, o brilho, a densidade e a fonte do tema chegam à sessão como os tokens deles, e saem com ela | `tema-sai-com-a-sessao.cjs` | `4ca29bd2` |
| 20 | o croma e o passo do preenchimento são os do texto da fórmula | `tons-do-tema.cjs` | `16b9b138` |
| 21 | a aparência substituída veta o tema de outro MOD, pelo nome, e a prévia não disputa | `tema-sai-com-a-sessao.cjs` | `cd99338f` |
| 22a | o papel que o MOD não pediu é `papel-nao-serve`, com o papel: a conferência sai do comando, que não sobe em teste | `main.rs` | `40c3d8c3` |
| 22 | as linhas do celular pela bancada que roda no CI desde `937d4251`, caso a caso; **um guarda de enfeite** (a rolagem do `operador.barra` passava com `overflow: visible`), e medidos a letra de 16 dos campos, o zoom e os gestos e o `status.barra` na casca larga; o descarte com a instância, sem pergunta; a duração de cada aviso; a revogação que recalcula do estado de agora; as capas animadas com mais de seis à vista | `tools/bancada-do-celular.py`, `contribuicoes-e-camadas.cjs`, `ajustes-v013.cjs`, `regioes-do-produto.cjs`, `capa.cjs` | `fa31a7d1` |
| 23 | cada ponto de contribuição põe o nó do MOD no lugar que promete; com a navegação e a lista substituídas, o ENTRAR, o renomear, o apagar e a moderação continuam do produto; o avatar que existe, citado. **Achado**: o `pessoa.detalhes` não tem lugar na linha nativa | `regioes-do-produto.cjs`, `capa.cjs` | `85a3292c` |
| 24 | o cabeçalho diz de qual MOD é; o foco entra no diálogo e volta ao acionador; o núcleo aguenta os três acentos que o texto nomeia, na janela e na faixa de voz | `nucleo.cjs`, `tools/bancada-do-celular.py` | `c6198b64` |
| — | **o achado do 23, consertado** (decisão da orquestradora): o `pessoa.detalhes` na linha nativa, num «⋯» próprio, com o nome do MOD por cima numa caixa que recorta, por um lugar só (`detalhesDaPessoaNaLinha`), e `CASCA.detalhesNaLinha` para o celular pô-lo na folha da pessoa (a F8) | `base.js`, `mods-controles.css`, `tela-sessao.js`, `regioes-do-produto.cjs` | `2ff19efb`, `701302c6`, `4aeab701` |
| 25 | a portaria, a moderação e o aceite à vista no guarda 11; a linha de dono do «⋯» descoberta, fora do tema e acima de 4,5:1; o «⋯» nativo pelo teclado; o palco pelo ponteiro de verdade | `nucleo.cjs`, `palco-da-chamada.cjs`, `regioes-do-produto.cjs` | `8f537bd8` |
| — | **dois consertos do lote 25** (decisão da orquestradora): a linha de dono no `--nucleo-rotulo`, 6,82:1, e não nos 4,11 do apagado (achado da F8); e **a portaria fora do alcance dos diálogos de MOD** — um diálogo (z 60) cobria o VER QUEM É e o DEPOIS (z 19), e com um modal eles ficavam inertes —, pelo recorte do modal, pela área do não modal e pela pilha de inércia, e não subindo o z dela, que cobriria as camadas do produto entre 20 e 40 | `tela-sessao.css`, `mods-controles.css`, `mods-superficies.js`, `mods-superficies.css` | `4e2e1497`, `87908e41` |
| 26 | o véu de quem fala, a capa lisa enquanto carrega, a grade rolada (as de fora paradas) e a citação pelo id na folha | `capa.cjs` | `da214aa8` |
| 27 | o movimento reduzido em toda região e camada — duas regras o entregam, e tirar as duas reprova | `regioes-do-produto.cjs` | `d319c7d9` |
| 28 | o braço `pedido` do roteador chama o servidor com o id do MOD dele | `contribuicoes-e-camadas.cjs` | `df7e947d` |
| — | o `superficie-classes` já estava provado na `camadas.cjs` (`2f76bc4c`, `71c976a7`, `5b7ff153`), e só não remarcado: as três reversões reprovam | — | — |
| 29 | a folha convive com o palco da chamada e com a página não imersiva | `palco-da-chamada.cjs`, `camadas.cjs` | `a0b67d34` |
| 30 | o SEU CÓDIGO e o «receber ligações» no guarda 11 | `nucleo.cjs` | `c0a2f7cc` |
| 31 | no aparelho: o `listarNaBarra`, a célula inteira e as áreas seguras de cada superfície (as laterais, pelo teto da coluna); o rodapé e a altura dos diálogos saíram, porque a F8 os mede (`cf031786`) | `tools/bancada-do-celular.py` | `2bb317de` |
| — | **a sala aberta** (8:78; D-api-10): a ocupação e os nomes no acento, que a derivação garante sobre o preenchimento | `tela-sessao.css`, `tema-claro-legivel.cjs` | `150020b6` |
| — | **a quarta e última emenda** (D-api-10): o `listarNaBarra` na aba SERVIDOR, o `pontoPedido` e os `pontos`, e `estilos` e `classes` como rótulos sem efeito (1:26, 1:27) | `api/v6.json` | `206d1b4b` |
| 32 | a `midia`, o retrato e o fundo dividem o bolso de bytes da superfície, pela soma | `regiao-do-mod.cjs` | `00a42671` |
| — | o servidor não confere contraste: um tema de forma válida e acento igual ao fundo passa nas duas pontas | `chaves-do-tema.json` | `a7766d6d` |

E o que o `.wasm` cobrava, rodado pela primeira vez nesta máquina com o
wasi-sdk conferido: dois consertos meus (a B2 e a B10) tinham deixado o
`motor.mjs` e a `executor-no-navegador.cjs` vermelhos, e as cobranças foram
postas na forma que o produto já tinha (`8e8dfea5`).

**Decididos pela orquestradora (16/10), consertos e não emenda:**
- o retrato e o `fundoDeMidia` recusados pelo bolso de bytes, ou com bytes que
  não são imagem, mandam `midia/recusada` ao MOD, com o porquê, como a
  `midia` (`b1a4c3cf`);
- o `ponto-desconhecido` leva ao MOD `pontoPedido` e `pontos`, no molde do
  `tipo-desconhecido` (`021876f8`). Os dois nomes só entram no `v6.json` se
  houver uma quarta emenda por outro motivo.

**O passo 7 está fechado (20/10/2026), e a tabela inteira remarcada.** A
contagem: **526 PROVADA e 25 NÃO SE APLICA**, das 551 linhas, sem nenhuma
INCERTA, SEM PROVA ou SEM CÓDIGO. Cada prova foi revertida contra o produto e
reprovou. O que pedia texto foi numa quarta e última emenda (`206d1b4b`);
depois dela, a v6 só muda na API 7.

As 35 SEM CÓDIGO, remarcadas a pedido da orquestradora (20/10), cada uma com o
commit que a fechou: 26 PROVADA pelos consertos da B (B1 a B10), pelo C6, pelas
entregas que a C decidiu (`0e9f8fbf`, `41cfbec5`, `c47b6874`), e pelo texto
que a primeira emenda corrigiu, provado hoje (o 7:81, a soma do bolso, pelo
lote 32, `00a42671`; o 8:80, o servidor que não confere contraste, pelo vetor
`a7766d6d`); e 9 NÃO SE APLICA, as cláusulas que a primeira emenda
(`44025ab5`) tirou do texto. As contagens no topo de cada tabela e em «A
contagem, em 48b6a1ba» são as do dia da auditoria, e ficam como registro.

**Ditos, fora da auditoria:** o «×» desligado da sala aberta fica a 1,32:1, e o
comentário dele agora diz que é inativo, dispensado pela WCAG 1.4.3
(`150020b6`); e as superfícies do celular não aplicam o inset lateral — deitado,
a área segura vale pelo teto da coluna, e a F8 vai pô-lo nas superfícies.

Os achados 1 e 2, as bancadas de navegador e a do celular no portão de
publicação, são da F6.
