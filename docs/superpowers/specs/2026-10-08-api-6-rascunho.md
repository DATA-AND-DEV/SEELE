# A API 6 de MODs — rascunho

> **Estado: rascunho, não congelado.** Nada aqui é um nome publicado. O
> `api/v6.json` e as constantes (`APIS_ACEITAS = [6]`, `MOD_API_VERSION = 6`)
> sobem **por último**, no fim do pacote da 0.16, depois da faixa de toque da
> sala pessoal (spec-mestre, F7). A partir daí a API 6 está congelada
> (`api/README.md`, «Publicar a versão seguinte»).
>
> O par legível por máquina deste documento é
> [`2026-10-08-api-6-rascunho.json`](2026-10-08-api-6-rascunho.json), **fora de
> `api/`**: o guarda `a_api_publicada_nao_se_edita` reprova uma `vN.json` em
> `api/` que não esteja em `api/congeladas.sha256`, e uma v6 listada ali já
> seria a construção da versão. Este rascunho vira `api/v6.json` na tarefa 12
> do plano da F3 (`docs/superpowers/plans/2026-10-08-0.16-F3-api-6.md`).

**Data:** 2026-10-08. **Frente:** F3 da 0.16 (spec-mestre
`docs/superpowers/specs/2026-10-08-0.16-design.md`, §2 F3 e F4, §3).
**Base do código:** `versao/0.16` em `92e0d79`. O código de `apps`, `crates` e
`api` é idêntico ao da v0.15.1 publicada (`54f542f`; conferido pela API de
releases do SEELE-RELEASES e por `git diff --stat 54f542f HEAD`, que só tem
documentos). As âncoras `arquivo:linha` valem para a versão que as pessoas têm.
**Insumos:** o levantamento da F3
(`docs/superpowers/sdd/2026-10-08-0.16/levantamentos/F3-api-6.md`), a Parte II
da spec de 23/09, os ADRs 0029, 0045, 0048, 0049, 0052 e 0057, as três
revisões dos MODs oficiais (`docs/revisao-0.16.md` no branch `revisao-0.16`
de PERFIS, MESA e ESTILO), e a seção «Para a F3» do `lotes.md` da F5 (branch
`0.16/lotes`, `46f227a`).

Marcas, como nos levantamentos: **[c]** lido no código; **[d]** o que um
documento afirma; **[inf]** inferência; **[medido]** rodado.

---

## 0. O que muda, numa página

1. **Só a API 6 roda no app 0.16.** Quem precisa de um MOD das APIs 3, 4 ou 5
   usa o Servidor 0.15 pelo launcher. **Servidor 1.0 = protocolo 1.0 + API 6.**
2. **A janela inteira vira mapa de regiões nomeadas**, com dois modos
   (`adicionar` e `substituir`). Os 11 pontos de hoje continuam com o mesmo
   nome e passam a morar dentro das regiões.
3. **`pessoa.capa`**: a capa de uma pessoa, no cartão da chamada, no fundo da
   barra do operador e nos dois cabeçalhos de perfil.
4. **Camadas**: `chamada.palco` (sobre a chamada), `folha` (coluna que
   empilha), `dialogo` com formulários declarados, e o `aviso` de sempre.
5. **O MOD deixa de ler o `Snapshot` inteiro.** Ele lê um **retrato projetado
   por lista branca** e assina **eventos da tela**. O veredito de conexão
   segura, os aparelhos da máquina e a telemetria saem do alcance dele.
6. **A metade de servidor do MOD avisa as janelas** de que um dado dela mudou.
   É o que tira a consulta em laço dos três MODs oficiais (2 s na MESA, 4 s no
   PERFIS e no ESTILO).
7. **Toda região e toda camada diz o que faz no celular** (D-web-1): se existe
   na casca de uma mão, com que tamanho, e o que acontece com uma contribuição
   para uma região que lá não existe.
8. **A v6 promete só o que é entregue**, e é escrita por inteiro, sem
   `extends`. O que as versões 1 a 5 prometiam e nunca entregaram não entra
   (§13).
9. **O núcleo de confiança vira contrato testado**, e passa a incluir o
   apelido verificado (D-nucleo-1), a faixa de voz e as abas do celular, e o
   contraste dos itens do núcleo sob qualquer tema.

---

## 1. O que não muda

Três decisões medidas continuam de pé, e o resto deste documento depende delas.

- **O MOD não volta para a janela** (ADR 0049). Ele roda no QuickJS nativo
  (`apps/seele-app/src/executor.rs`), sem documento, e no celular roda num
  executor isolado da origem da página (spike da F4, §16).
- **Sem HTML livre e sem seletor de CSS** (ADR 0052). O MOD declara, e o
  produto monta com `createElement` e `textContent`. Uma região é endereçada
  por nome, que é um contrato versionado.
- **Identidade antes de apresentação.** Trocar o que se desenha nunca troca a
  chave. Com a D-nucleo-1, isto passa a valer também para quem **lê**: o
  apelido verificado fica à vista (§10).
- **A API é uma fachada congelada** (ADR 0045, `api/README.md`). O nome que o
  MOD escreve não muda quando o interior muda.

## 2. Compatibilidade e capacidades

**Aceitas: `[6]`.** O `ModAnunciado` ganha `api`, e o `Hello` leva as APIs que
o cliente roda (D-fio-7, item 15 do contrato de fio). Um servidor 1.1 com um
MOD da API 7 recusa, antes do convite, o cliente 1.0 que não a roda, e a recusa
diz «atualize o app».

**Um pacote das APIs 3, 4 ou 5** é recusado com `ApiTooOld`. A frase de tela
(`camada-mods.js:1673-1685` [c]) passa a apontar o Servidor 0.15 no launcher.
O catálogo continua escolhendo, por `ultimaQueEsteBuildEntende`
(`camada-mods.js:1878` [c]), a versão de cada MOD feita para a API 6.

**As capacidades da API 6** (`capacidades_da_api(6)`, `mods.rs:88-113` [c], e
o espelho `CAPACIDADES_POR_API` em `base.js:1982-1989` [c]):

| capacidade | na 6 | por quê |
|---|---|---|
| `superficies` | sim | `pagina`, `painel`, `dialogo`, `aviso`, `folha` (§5) |
| `contribuicoes` | sim | as regiões e os pontos (§3) |
| `estilos`, `classes` | sim | sem mudança de contrato |
| `volume` | **sim** | ADR 0048. O PERFIS (R1, R3), a MESA (M11, item 16) e o ESTILO (§1, fundo por mídia) dependem dela. Confirmado à F5 em 08/10: o lote E (P51-11) tem objeto |
| `arquivo` | sim | o seletor mediado e `pedaco`/`soltar` |
| `tema` | sim, ampliado | §9. **Em aberto para o dono e as sessões de MOD**, com recomendação |
| `regiao` | **recomendo que saia** | a faixa de 240 px é o U01–U03 da auditoria de 20/09 (ADR 0052). Nenhum oficial a pinta: os três calculam `aRegiao` e jogam fora (PERFIS B4, MESA B3, ESTILO A19). **Em aberto** |
| `cartoes` | **recomendo que saia**, trocada por `pessoa.cartao` com `alvo` | o conjunto inteiro vai numa mensagem de 12 KiB e congela a lista com 14 pessoas (PERFIS A1, R2). **Em aberto** |
| `eventos-da-tela` | sim, nova | §7 |
| `aviso-do-servidor` | sim, nova (metade de servidor) | §7.3. Depende do fio (§14) |

**Capacidade por plataforma, e não só por versão.** Hoje a ausência é por
versão: um pacote de API 3 recebe um `SeeleUI` sem `superficies`
(`comCapacidade`, `executor.rs:1643` [c]). Na 6, o executor do celular omite
também o que a ponte web não tem (PERFIS R13). Se ela não tiver
`enviar_imagem_mod`, o `SeeleUI.enviar` não existe lá, e o MOD mostra o editor
sem os controles de imagem, com uma frase. **Ausente, e não recusado depois de
a pessoa escolher a foto.**

**O guarda que nunca existiu.** O comentário de `base.js:1963-1965` [c] cita
`a_casca_confere_as_mesmas_capacidades_que_o_nucleo`. O teste não existe: o
`git log -S` só acha o comentário (`26ad0c2`). A API 6 o cria: `CAPACIDADES_POR_API`
na casca e `capacidades_da_api` no Rust são comparados por teste.

## 3. Regiões e modos

O MOD continua declarando o que quer ver, e o produto continua montando. Muda o
alcance: da lista de convites para a janela inteira. `decorar` não entra: é a
API 7, somada depois (decisão de 29/09), e o guarda
`decorar_nao_volta_a_tabela_sem_quem_o_aplique` continua.

O mecanismo é o de hoje: cada região vira uma linha de
`PONTOS_DE_CONTRIBUICAO` (`mods-contribuicoes.js:76-88` [c]), de
`CONTEINERES_DOS_PONTOS` (`camada-mods.js:1044` [c]) e de `NOMES_DOS_PONTOS`
(`camada-mods.js:832` [c]). Os guardas de `frontend.rs` já cobram linha nova
(quem aplica, onde contornar, se `substituir` é consultado).

| região | área no desktop | modos | pontos que moram dentro | o que continua do produto |
|---|---|---|---|---|
| `redes` | a trilha de servidores (`index.html:941-973`) | adicionar | — | o servidor atual e «conectar a outro» |
| `servidor.navegacao` | salas, canais e entradas | substituir, adicionar | `canal.item`, `sala.acoes` | ENTRAR NA SALA e a moderação de salas e canais |
| `operador.barra` | a barra de quem está conectado (`.operador`, `index.html:1064`) | adicionar | `pessoa.capa` (de quem usa) | **SAIR, microfone e «no ar»** (núcleo) |
| `canal.cabecalho` | o topo do canal (`#canal-cabecalho-mods`, `index.html:1424`) | substituir, adicionar | — | o nome do canal e a busca |
| `conversa.mensagem` | cada mensagem (`#lista-mensagens`, `index.html:1471`) | substituir, adicionar, por mensagem | `pessoa.avatar` | **o autor continua sendo a chave**, e o apelido dele fica à vista (§10) |
| `compositor` | a caixa de escrever | adicionar | `compositor.ferramentas` | ENVIAR e o anexo |
| `pessoas.lista` | a faixa de pessoas | substituir, adicionar | `pessoa.identidade`, `pessoa.cartao`, `pessoa.detalhes`, `pessoa.acoes`, `pessoa.avatar`, `pessoa.capa` | **o menu de moderação**, e o apelido verificado |
| `status.barra` | a barra inferior (`footer.telemetria`, `index.html:1761`) | adicionar | — | **atraso, jitter, distúrbio e codec** |

**Os 11 pontos de hoje ficam com o mesmo nome**, e `servidor.aparencia` passa a
ser a porta do tema (§9). `pessoa.avatar` existe desde `14d9c30` (22/09) e
saiu na 0.15.0, mas **nunca entrou num `vN.json`** (§13). A v6 o lista.

**Uma substituição de região inteira não leva os pontos de dentro.** Quem
substitui `pessoas.lista` desenha a lista, e o produto continua aplicando
`pessoa.avatar` e `pessoa.capa` de quem os registrou, e continua desenhando a
moderação e o apelido verificado de cada linha (§10). [inf] Isto é o que
mantém duas garantias juntas: a de identidade (o menu age pela chave) e a de
leitura (D-nucleo-1). **A forma exata de uma região substituída que ainda
carrega os itens do núcleo é a primeira coisa a desenhar na tarefa 5 do
plano.**

**Um tipo de superfície ou um ponto desconhecido é recusado pelo nome.** Hoje
um tipo desconhecido vira `dialogo` em silêncio
(`mods-superficies.js:225` [c]). Na 6, a recusa diz o nome e a lista do que
existe, e o `seele.log` ganha a linha (o «produto sabe e não conta» do
`CLAUDE.md`).

## 4. `pessoa.capa`

**O nome é `pessoa.capa`** (D-api-1). `pessoa.faixa`, o nome da spec de 23/09,
já é a faixa de sincronia no roster (`tela-sessao.js:1799`, `:2198` [c]) e no
cartão da chamada (`data-faixa`, `tela-chamada.js:454` [c]), e «banner» é o
`#banner` de alerta (`index.html:631` [c]). O atributo da marcação também tem
nome próprio: `data-pessoa-da-capa`, nunca `data-faixa` (PERFIS R1).

**A forma é a do `pessoa.avatar`**: `substituir`, por pessoa (`alvo`), com
`conteudo.doServidor` (`mods-contribuicoes.js:78`, `:165-171` [c]). O
mecanismo a copiar também: `avatarContribuido` (`base.js:1179-1228` [c])
carrega uma vez por contribuição e guarda; `vestirAvatar` e `aplicarAvatar`
(`tela-sessao.js:4509-4530` [c]) marcam e aplicam; `redesenharAvatares`
reaplica quando a mídia chega. A capa ganha `capaContribuida`, `vestirCapa` e
`aplicarCapa`. O GIF já é aceito (`mods-regiao.js:358` [c]).

```js
SeeleUI.contribuicoes.registrar({
  ponto: 'pessoa.capa', modo: 'substituir', alvo: id, prioridade: 10,
  conteudo: { doServidor: { canal, pedido, campo } },
});
```

**Onde ela aparece** (decisões de 29/09 e D-capa-1):

| lugar | desktop | celular |
|---|---|---|
| o cartão da pessoa na chamada (`pintarCartao`, `tela-chamada.js:453` [c]) | sim, animada até o teto | **sim, animada até o mesmo teto** (D-capa-1) |
| o fundo da barra do operador (`.operador`, com `snapshot.me`) | sim | **não**: a barra sai da vista, e a faixa de voz é do núcleo |
| o cabeçalho do perfil do PERFIS | sim, por citação (abaixo) | sim |
| o «SEU PERFIL» nativo (`index.html:3816`) | sim, por um aplicador do produto | sim |
| a linha da lista de pessoas | parada, se o MOD a citar no cartão | parada |

**Citar a capa já carregada, em vez de repetir o `doServidor`.** No cartão da
lista e na página de perfil, o MOD escreve `fundoDeMidia: { daPessoa: 'capa' }`
ou `retrato({ daPessoa: 'avatar' })`. A imagem é carregada e decodificada uma
vez, conta uma vez no bolso, e o cartão perde os ~340 bytes de `doServidor`
que hoje gasta da mensagem de 12 KiB (PERFIS R2).

**A preferência de quem administra herda** como a do avatar herda a do cartão
(`camada-mods.js:632-642` [c]): quem escolheu «o SEELE desenha» para as pessoas
não recebe capa de MOD. Se a linha nativa da lista ganha capa nesse caso: **não**
(proposta do PERFIS, que adoto).

**Legibilidade, com critério medido.** A capa fica por trás do nome, da frase
de estado, das pastilhas MUDO/FALANDO/OUVINDO/SEM SOM, do número e da barra de
sinal, e do volume. O PERFIS calculou o contraste dos tokens reais sobre o pior
quadro com um véu uniforme (R1, tabela): só um véu de 0,9 segura o laranja e o
vermelho perto do nativo, e aí a capa quase não se vê. O contrato:

- **o texto do produto fica em placas opacas** do negro do painel, ou num véu
  opaco onde há texto. A capa aparece no resto;
- **todo texto do produto sobre a capa mantém pelo menos o contraste que tem
  sem capa**, conferido contra um quadro branco e um quadro preto, com guarda;
- **MUDO, fone e SAIR nunca ficam sobre a capa** (núcleo, §10).

**Quem fala.** O halo `inset` e a pastilha FALANDO ficam acima da capa, e
quando a pessoa fala o véu passa a `--seele-laranja-carga`, para o
preenchimento continuar dizendo «falando» (`tela-chamada.css:409-412` [c]).

**Sem capa, carregando, recusada.** O cartão fica como hoje: fundo liso, sem
lugar reservado nem brilho de «carregando». Recusada: fundo liso, uma linha no
`seele.log` com o motivo, e o evento `midia` ao MOD **com o ponto e o alvo**
(§7.1). Revogada ou MOD desligado: o produto recalcula do estado atual. A capa
carregada fica no cache da contribuição: não pisca entre redesenhos.

**Movimento reduzido.** `animation: none` (`mods-superficies.css:423-429` [c])
não para um GIF, e só vale em `.superficie-de-mod` e `.pessoa-cartao`. O
produto guarda **um quadro estático por capa** (o primeiro, por `drawImage`
num `canvas`), usado com `prefers-reduced-motion`, com o movimento reduzido do
próprio SEELE, com a janela oculta, com o cartão fora da vista e acima do teto
de capas animadas.

**O custo é dos quadros decodificados, e não dos bytes do arquivo.** Hoje os
bolsos somam `midia.bytes` (`base.js:1209` [c]). Uma capa de 960 × 320 tem
1,23 MB por quadro; 60 quadros, 74 MB (conta do PERFIS, não medida). O
contrato, **com os números em aberto para a M3** (§12):

- o Rust **reduz** a capa ao maior tamanho em que ela aparece antes de
  entregá-la à janela, e limita quadros e duração (proposta: 60 quadros, 10 s);
- o bolso da capa conta **bytes decodificados**;
- **no máximo N capas animadas ao mesmo tempo** (proposta: 6, contando a do
  operador), escolhidas pela vista: as visíveis, e entre elas quem falou por
  último. O teto não depende do tamanho da sala (até 250 pessoas,
  `control.rs:77` [c]).

## 5. Camadas e superfícies

| camada | o que é | modos ou tipo | celular |
|---|---|---|---|
| `chamada.palco` | sobre a chamada, com a grade de vídeo do produto por baixo (`#chamada-grade`, `index.html:1407` [c]) | ponto de perfil `superficie`, `substituir` e `adicionar` | **vista alternativa** à grade (MESA · CHAMADA), com quem fala numa tira no alto |
| `dialogo` | modal; a saída é do produto (`mods-superficies.js:190` [c]) | superfície | **tela cheia**, com as ações fixas embaixo, acima da barra de gestos |
| `folha` (nova) | coluna lateral que empilha, para edição e detalhes; parente do `painel` (`#palco-de-paineis`, `index.html:1702` [c]) | superfície | **folha de baixo**: sobe da borda até onde o polegar alcança e se expande à tela inteira |
| `aviso` | fila de quatro, como hoje (`mods-superficies.js:206` [c]) | superfície | canto de baixo, com a área segura |
| `pagina`, `painel` | como na API 4 | superfície | ver «Em aberto», abaixo |

### `chamada.palco`

- **Contexto.** A chamada é de uma sala de voz, e um pedido do MOD é por canal
  (`SeeleMods.request(id, canal, valor)`, `executor.rs:1615` [c]). Um pedido
  feito do palco diz **de que sala veio** (`contexto.sala`), e o retrato diz a
  sala ocupada (§6). Não amarro sala a canal na API: é o MOD que decide (MESA,
  item 5a).
- **O que aceita**: a gramática inteira da superfície, inclusive `tela` com
  traço e arraste e `midia` de fundo. **Recebe toque e clique**, porque é
  superfície e não ponto de cartão.
- **De quem.** O palco é **por pessoa**: cada janela monta o que o MOD dela
  declarar. Quem mestra vê peças ocultas e notas; quem joga não (MESA, 5c). Um
  palco compartilhado seria estado de servidor, e o MOD já o tem.
- **Tamanho.** O MOD recebe a largura e a altura que tem ali, e um evento
  quando elas mudam (§7.2); as `classes` com `consultas: [{ ateLargura }]`
  valem dentro do palco.
- **Saída.** Como toda superfície: a saída é do produto, e a grade de vídeo
  continua de pé por baixo. A faixa de toque e a de batida ficam fora, por
  construção (§10).

### `folha`

- Empilha **até 3** [proposta], cada uma com título e a saída do produto.
- Convive com `chamada.palco` e com uma `pagina` não imersiva. Sob uma página
  imersiva, a folha abre por cima dela.
- `fechar` vai ao MOD como nas outras, com `descartou` quando a pessoa aceita
  perder o rascunho (o contrato de M3 do PERFIS e de A1 do ESTILO continua).
- No celular, o teclado do sistema empurra a folha e o diálogo para cima: o
  campo com foco nunca fica embaixo do teclado.

### `dialogo` com formulários declarados

Hoje existem `formulario`, `acoes` e `submeter` (`mods-regiao.js:274`, `:825`,
`:1633-1678` [c]), mas **o produto não valida nada**: o MOD declara `erro` por
controle, `botao` é sempre `type="button"` e o envio só acontece por Enter num
campo. A API 6 acrescenta:

- **validação declarada por campo**: `obrigatorio`, `minimo`/`maximo` (número),
  `tamanhoMaximo` com contador e frase do produto, `inteiro`. Uma regra
  violada impede o envio e marca o campo, sem ida ao MOD;
- **`erro` em toda forma de entrada**, inclusive `cor` e `escolha`
  (hoje só `campo`, `textoLongo`, `numero` e `formulario`, ESTILO §4);
- **botões primário e secundário montados pelo produto**: o primário envia o
  formulário com **todos os valores juntos** no `submeter`; o secundário
  cancela pela saída do produto. O MOD dá o texto, não o comportamento;
- **a recusa do servidor volta ao campo que a causou**:
  `formulario.erros({ chave: 'motivo' })`;
- **`sujo` derivado**: o produto compara com o valor inicial, e a pergunta ao
  fechar não depende de o MOD chamar `suja`;
- **confirmação declarada para ação destrutiva**, com as palavras do produto:
  `{ confirmar: 'isto muda a aparência para todo mundo neste servidor' }`
  (ESTILO A13);
- **`emProgresso` no primário trava o segundo clique** (ESTILO A6);
- **tela cheia com ações fixas** quando o produto decide (celular, ou
  `largo: true` numa janela estreita), com `abas` cabendo em 390 px
  (D-estilo-1).

### O que fica como está

O `aviso` (fila de quatro, 3,5 s e 10 s) e as garantias de saída do ADR 0052.

### Em aberto: `pagina` e `painel` contra `folha`

**Para o dono e as sessões de MOD.** Os três oficiais usam `pagina` e `painel`
hoje (PERFIS diretório e editor, MESA a mesa, ESTILO o diálogo). Três
caminhos:

1. **ficam as cinco** (`pagina`, `painel`, `dialogo`, `aviso`, `folha`). O
   `painel` e a `folha` são parentes e quase redundantes;
2. **`folha` substitui `painel`**: um tipo a menos, e a coluna que empilha cobre
   o que o painel fazia;
3. **`folha` substitui `painel` e `pagina` vira só imersiva.**

**Recomendo o 2.** A spec de 23/09 já desenha a `folha` como «parente do
painel», o porte é obrigatório de qualquer jeito (API-6-só), e um tipo a menos
é uma superfície a menos para revisar e para descrever no celular. A `pagina`
fica, porque a MESA imersiva e o diretório do PERFIS precisam da célula da
conversa.

## 6. O retrato projetado por lista branca

**Hoje o MOD lê o `Snapshot` inteiro.** O prelúdio define
`SeeleMods.snapshot: () => pedir('snapshot', {})` (`executor.rs:1616` [c]), e
a janela responde com `invoke("snapshot")` sem filtro
(`base.js:2126-2133` [c]), em qualquer API. Isso entrega ao MOD, entre outras
coisas (`crates/seele-ffi/src/types.rs:1185-1424` [c]):

- `link_state`, que é **o veredito de conexão segura**, do núcleo;
- `caminho`, que revela a topologia de rede (rede local, IPv6, endereço
  público, furo de NAT);
- `capture`, `playback`, `aparelho` e `trocas_de_aparelho`: **os nomes dos
  aparelhos da máquina** (nota da F5);
- `telemetry` e `notice`.

E a MESA lê desde 17/09 um `open_channel` que **nunca existiu** (MESA A1): o
canal aberto é o que tem `open`. Um retrato escrito no contrato acaba com
isso.

**Na API 6, `SeeleMods.snapshot` sai e entra `SeeleUI.retrato()`**, com estes
campos e só estes:

| campo | o que é | de onde (`types.rs` [c]) |
|---|---|---|
| `eu` | o id de quem usa | `me` |
| `servidor.nome` | o nome do servidor | `server` |
| `pessoas[]` | `id`, `apelido`, `falando`, `mudo` de quem está no servidor | `presentes` (`Person`, :494) |
| `salas[]` | `id`, `nome`, `limite`, `pessoas` (ids), `minha` | `voice_rooms` (`VoiceRoom`, :549) |
| `canais[]` | `id`, `nome`, `aberto` | `channels` (`Channel`, :579) |
| `onde` | `computador` ou `celular` | a plataforma da casca (`data-plataforma`) |
| `regioes` | as regiões e camadas que existem nesta casca agora | a tabela do §11 |

`falando` entra porque **o indicador de quem fala não é núcleo** (decisão de
29/09): é região, e um MOD pode redesenhá-lo.

**Ficam de fora, e por quê:**

| campo | por quê |
|---|---|
| `link_state`, `link` | o veredito é do núcleo (§10) |
| `caminho` | topologia da rede de quem usa |
| `capture`, `playback`, `aparelho`, `trocas_de_aparelho`, `voice_mode`, `audio_available`, `total_isolation` | a máquina de quem usa |
| `telemetry` | atraso, jitter e codec continuam do produto (`status.barra`) |
| `notice` | aviso do produto |
| `tela`, `minha_transmissao`, `transmissoes` | nenhum oficial pede; entra numa API somada, se alguém pedir |
| `may_*` | a permissão vale na metade de servidor (`contexto.admin`, `contexto.write`), e não na janela |
| `messages_revision`, `icon_revision`, `person_icons_revision`, `ended` | estado interno da casca |
| `membros` (o FORA DO AR: `Membros`, `MembroNovo`, `MembroBanido`) | **recomendo que fique de fora na 6** (abaixo). Decisão do dono |
| **toque, chamada, código, agenda** | nunca (Parte II, «Onde as duas partes se tocam»). O guarda 12 prova |

**`reads` lista cada campo**, e o `check-api` passa a cobrar que cada um tenha
quem o projete. Um retrato que cresce é uma API somada.

### Os membros fora do ar

A F1 pôs no protocolo o FORA DO AR (`Membros`, `MembroNovo` e `MembroBanido`,
ordinais 42 a 44, com `id` e `nickname`, sem papéis e sem horário; D-fio-4).
**Recomendo que o retrato da 6 não os leve: ele leva só quem está presente.**

- **A assimetria do congelamento.** Acrescentar `membros` depois é uma API
  somada (a 7), que não quebra nada. Tirar depois é impossível: o nome congela
  com o `v6.json`. Na dúvida, a porta que se pode abrir depois fica fechada.
- **Nenhuma sessão de MOD pediu os ausentes.** O PERFIS pede `me` e «as pessoas
  do servidor, com `id` e apelido» (R4) e a MESA pede «quem está no servidor»
  (item 3), os dois lendo `presentes` hoje.
- **Os casos que pediriam têm o dado do próprio MOD.** O diretório do PERFIS
  lista os perfis que a metade de servidor dele guarda, por `id`, estejam as
  pessoas presentes ou não. A MESA, para dar uma ficha a quem joga e não está,
  tem a lista de quem já entrou na campanha (a metade de servidor recebe
  `PersonJoined` com o apelido, §7.4).
- **O MOD não é «todos».** A D-fio-4 diz que todos os presentes veem a lista,
  na tela do produto. Um MOD é código de terceiro, com rede de saída na metade
  de servidor (`mundo.buscar`). O que ele ganharia é **o rol de membros do
  servidor**, inclusive de quem não entra desde que o MOD foi ligado, e ele
  poderia mandá-lo para fora. O `specs/00-visao-geral.md` chama a presença
  publicada de defeito do formato que o SEELE recusa.

**Se o dono quiser os membros na 6**, a forma é `membros[]` com `id` e
`apelido` e nada mais: sem horário, sem estado («fora do ar desde»), sem ordem
por última vez visto, e o `pessoas[]` continua sendo a presença. E o evento
`pessoas` (§7.2) não ganha `membroNovo` nem `banido`.

**Isto não segura a tarefa P6 da F1** (o FORA DO AR na casca): o retrato é uma
projeção por lista branca (tarefa 4 do plano), e um campo que a lista não copia
não chega ao MOD, qualquer que seja a forma do `Snapshot`.

## 7. Eventos e momentos

### 7.1 Os eventos de controle (janela → MOD, por `aoEvento`)

Os 16 da v5 (`api/v5.json:79-96`) mais **`contribuicao`**, que a janela já
despacha (`base.js:1331-1336` [c]) e a v5 não lista (§13). A extração textual
que o `check-api` usa acha exatamente esses 17 na HEAD [c].

Mudam, na 6:

- **`midia`** passa a dizer **o ponto e o alvo** (`{ ponto: 'pessoa.capa', alvo:
  12, estado: 'recusada', porque }`), para o MOD dizer a quem enviou que a capa
  não vai aparecer (PERFIS M5, R9). E o retrato que não decodifica manda
  `midia`/`falhou` (nota do K2 da F5).
- **`traco`** ganha a fase `cancelou`. Hoje `pointercancel` chega como
  `terminou` (`mods-regiao.js:1898` [c]) e um gesto cancelado move a peça
  (MESA M14). No celular é comum.
- **`contribuicao`** ganha o motivo `regiao-ausente` (§11).

### 7.2 Os eventos da tela (novos)

O MOD assina mudanças em vez de consultar o retrato em laço. Cada evento leva
os mesmos nomes do retrato (§6), para o laboratório de cada MOD ser gerado do
contrato (MESA, item 3).

| evento | quando | carga |
|---|---|---|
| `canal` | o canal aberto mudou | `{ canal }` |
| `sala` | quem usa entrou numa sala de voz ou saiu dela | `{ sala | null }` |
| `pessoas` | alguém entrou no servidor ou saiu | `{ entrou?: id, saiu?: id }` |
| `fala` | o conjunto de quem fala mudou | `{ falando: [id] }`, no máximo 4 por segundo |
| `lugar` | a região ou camada do MOD mudou de tamanho, ou a casca mudou de forma | `{ onde, regioes, largura, altura }` |

**Nunca carregam a ligação.** O MOD do servidor em que a pessoa está não fica
sabendo que ela recebe uma ligação, de quem, nem qual é o código de ninguém.

### 7.3 O aviso da metade de servidor às janelas (novo)

Os três oficiais consultam em laço porque não há outro jeito: a MESA a cada
2 s (até 190 KB desserializados por `view`, ~20 MB por hora de resposta numa
mesa parada no celular, MESA M6 e item 17), o PERFIS e o ESTILO a cada 4 s.
A API 2 escrevia «Poll for updates» (`api/v2.json`). Os três pedem a mesma
coisa:

```js
// metade de servidor
mundo.avisar({ canal, carga });     // canal opcional: sem ele, o servidor inteiro
// metade de janela
SeeleUI.aoEvento((e) => { if (e.nome === 'servidor') { /* e.carga */ } });
```

- **Só para quem pode ler aquele canal**, e só para as janelas do mesmo MOD.
- **Teto de tamanho e de taxa** [em aberto para a M3]: proposta, 1 KiB por
  aviso e 4 por segundo por MOD; o excedente é contado e dito no `seele.log`.
- **Precisa de uma mensagem nova no fio** (servidor → cliente). Ver §14: é um
  item a acrescentar ao contrato de fio da 0.16, ou isto vira API 7.

### 7.4 Os momentos (metade de servidor, por `aoAcontecer`)

`momento_de` (`crates/seele-server/src/mods/despacho.rs:463` [c]) entrega 5:
`PersonJoined`, `PersonLeft`, `MessageReceived`, `MessageEdited` e
`MessageRemoved`. **A v6 lista esses 5 e só esses.** Os outros 16 da v1 nunca
foram entregues (`api/README.md`, «Os quatro blocos»). O `check-api` cobra os
dois sentidos desde que o arquivo existe (`check_api.rs:78`, `:186-211` [c]).

## 8. A metade de servidor

**A v6 nomeia o que o MOD escreve.** O runtime do servidor expõe
(`crates/seele-server/src/mods/mod.rs:505-637`, `:699`, `:750` [c]):

| nome | o que é |
|---|---|
| `aoPedir(contexto, pedido)` | `contexto = { person, channel, admin, write }` (`pedidos.rs:225` [c]) |
| `aoAcontecer(momento)` | os 5 momentos (§7.4) |
| `dados` | o quintal chave→valor, 256 KiB somados (`mods/mod.rs:649` [c]) |
| `arquivos.ler`, `.escrever`, `.listar`, `.apagar` | a pasta do MOD, 4 MiB por arquivo |
| `volume.esperar`, `.tamanho`, `.servir`, `.apagar` | ADR 0048 |
| `mundo.buscar`, `.agora`, `.registrar` | rede de saída, relógio e registro |
| `mundo.avisar` (novo) | §7.3 |

**O que as versões 1 a 5 diziam e não é isto** (§13): os 13 verbos de `actions`
da v1 (`mandarMensagem`, `criarSala`, `expulsarPessoa`…) e as 19 leituras de
`reads` (`servidor.pessoas`, `pessoa.apelido`…) **não são expostos à metade de
servidor**: nenhum desses nomes aparece no código fora de `api/` [c]. E o
bloco `own` da v1 chama de `guardar`, `lerArquivo` e `escreverArquivo` o que o
MOD escreve como `dados[chave]`, `arquivos.ler` e `arquivos.escrever`. O
`check-api` passava porque confere o símbolo da direita (`SendMessage`,
`recolher_quintal`), que existe.

**Pedido do PERFIS (R5), em aberto:** pedidos endereçados ao servidor, e não
ao canal, com uma permissão de servidor. Hoje todo pedido precisa de um canal
de texto (`ModRequest` leva o canal), e num servidor só de voz o PERFIS não
mostra nada. **Muda o fio** (§14).

**Pedido da MESA (item 9):** o orçamento do quintal legível pela metade de
servidor (`dados.uso()`, `dados.teto`), e a recusa por quintal cheio com código
próprio. Entra: é só fachada.

## 9. O tema

O tema da API 4 tem dez nomes escritos em `#tela-sessao` (`base.js:1876-1888`
[c]) e conferidos só como texto sobre fundo (`oTextoAlcancaOFundo`,
`base.js:1928` [c]). O ESTILO mediu o que isso deixa passar (A4).

**O que entra na 6:**

- **o contraste do núcleo é garantido pelo produto, com qualquer tema** (achado
  do ESTILO, spec-mestre §8). Os itens do núcleo leem os tokens do produto num
  escopo que o tema não sobrescreve. Um guarda prova que os acentos `#331704`,
  `#050403` e `#0a0806` não mudam a cor computada do `#veredito`, do
  `#botao-mudo` apertado e, no celular, do `#voz-mudo` e do `#voz-sair`;
- **contraste conferido em todos os pares que o produto desenha**, e não só
  texto sobre fundo: acento sobre `acentoFraco`, apagado sobre painel, texto
  sobre painel. A recusa diz o par e a razão;
- **tons derivados pelo produto** (`acentoFraco`, `bordaForte`, o segundo
  `apagado`), calculados a partir dos dez e conferidos por ele;
- **`servidor.aparencia` é a porta do tema.** Registrar o ponto passa a ser a
  condição para pedir tema, e a escolha «o SEELE desenha» **tira** o tema já
  aplicado (hoje só recusa o próximo, `camada-mods.js:982-985` [c]). É também
  acessibilidade;
- **a prévia é do produto**: `tema(valores, { previa: true })` vale só nesta
  máquina, aparece com o selo «prévia não publicada» e sai sozinha quando a
  superfície que a pediu fecha. No celular, «ver na sessão» recolhe o diálogo
  e mantém o selo (D-estilo-1);
- **`folha` e `dialogo` herdam o tema da sessão**, e um estilo de MOD pode
  citar um token (`cor: { tema: 'acento' }`), resolvido pelo produto (ESTILO
  A16).

**Em aberto, com recomendação, para o dono e o ESTILO:**

- **o tema como dado do servidor**, aplicado pelo produto antes do primeiro
  quadro e tirado na saída. Recomendo que entre: tira a ida e volta e põe a
  saída do tema na mão do produto por construção (o A3 da v0.15.0). **Muda o
  fio** (o tema iria junto com a entrada na sessão, §14);
- **tokens por região** (`regioes: { 'status.barra': { painel } }`): recomendo
  que entre, com a mesma conferência por região;
- **`modo: 'claro' | 'escuro'`**, tipo dos títulos separado do dos dados,
  escala por escolha, densidade que cobre a escala `--seele-e*`, e **fundo da
  sessão por mídia**. Recomendo `modo` e a densidade completa na 6; o fundo por
  mídia **depois da M3 no celular** (custo de memória não medido num iPhone, e
  o ESTILO aceita que o produto o dispense lá).

**Não entra, por nenhum caminho de tema:** `--seele-vermelho-alerta`, família
livre, `url(…)`, CSS em texto, z-index, opacidade ou deslocamento sobre
controle nativo, e esconder uma região. Os quatro últimos são o contrato do
`decorar`, que é a API 7.

## 10. O núcleo de confiança

**Nenhum item mora dentro de uma região, aceita contribuição ou é alcançado
pelo tema.** Fora deles, a tela é do MOD.

| item | por que é do produto |
|---|---|
| **Sair** (do servidor, de uma página, de um diálogo, de uma folha) | funciona mesmo com um MOD quebrado |
| **Microfone e «no ar»** | quem transmite precisa saber, sem depender de MOD |
| **Moderação por identidade** | o menu age sobre a pessoa pela chave |
| **O apelido verificado à vista** (D-nucleo-1, PERFIS R10) | quando um MOD troca o nome que aparece, o apelido verdadeiro fica ao lado, num lugar que o MOD não cobre. A moderação protege a ação; isto protege a leitura |
| **Atribuição** | cada superfície diz de qual MOD é |
| **Aceite** | a tela do «sim» ao conjunto de MODs |
| **A ligação** (Parte I) | a faixa «está ligando», CHAMANDO, SEU CÓDIGO, «não receber ligações» e a faixa de quem bate à porta |
| **Conexão segura** | o veredito: PRIMEIRO CONTATO, «o convite confirmou», CONEXÃO SEGURA, «QUEM ATENDEU NÃO É O DONO DESTE CÓDIGO» |

**No celular, além dos oito** (ESTILO §8):

- **a faixa de voz** (`.voz-celular`: a sala, MUDO, SURDO, SAIR), que é a forma
  do microfone e do sair na casca de uma mão;
- **as abas do pé** (SALAS · CONVERSA · PESSOAS · AJUSTES): AJUSTES é o único
  caminho para desligar um MOD. Elas podem tomar as cores do tema, com os
  pares conferidos, e não aceitam contribuição;
- **a pergunta «DE ONDE?»** (`#origem`) e o seletor do sistema;
- **as áreas seguras**: nenhuma superfície de MOD desenha embaixo do entalhe,
  da ilha ou da barra de gestos;
- **44 px e 16 px**: nenhum estilo de MOD desce um controle abaixo de 44 px nem
  um campo abaixo de 16 px de letra;
- **o zoom e os gestos** (`touch-action: manipulation`): nenhum MOD os religa.

**O guarda 11** prova, para cada item, que ele não está dentro de nenhuma
região, que nenhuma contribuição o cobre e que o tema não o alcança. Agora
para o que existe (a portaria, `#portaria-batendo`, `index.html:192` [c]); o
item «a ligação» (`#toque`) fecha depois da 7d da sala pessoal. **É isso que
põe o congelamento no fim do pacote** (spec-mestre, F7).

## 11. O celular

**Cada região e camada diz se existe na casca de uma mão** (≤ 600 px,
`arquivo/mobile-ios:apps/seele-app/ui/celular.css`, `8632668`), com que
tamanho, e o que acontece com uma contribuição para onde ela não existe. A
casca diz de si «não há uma casca de celular, há a mesma casca numa tela
estreita» (`celular.css:1-18`).

| região ou camada | no celular | tamanho | contribuição |
|---|---|---|---|
| `redes` | existe: faixa deitada no alto (`celular.css:220-224`, `:355-387`) | itens de 56 × 44 px, rolando para o lado | aplicada |
| `servidor.navegacao` | gaveta SALAS, que ocupa a célula inteira (`:397-407`) | largura da tela; controles de 44 × 44 | aplicada; as entradas «DOS MODS» vão na gaveta |
| `operador.barra` | **fora da vista** (`:231-234`): mora no pé da gaveta SALAS, e a faixa de voz faz o papel dela | — | **recusada com `regiao-ausente`**, e `pessoa.capa` do operador não se aplica |
| `canal.cabecalho` | existe, compacto (`:558-561`) | botões de 44 px | aplicada |
| `conversa.mensagem` | existe | — | aplicada |
| `compositor` | existe; campos de 16 px | botões de 44 × 44 | aplicada |
| `pessoas.lista` | gaveta PESSOAS, célula inteira (`:397-407`) | cartão de 44 px | aplicada; a capa da linha fica parada |
| `status.barra` | **só na chamada** (`#tela-sessao:has(#vista-chamada[hidden]) .telemetria{display:none}`) | mínimo de 40 px | aplicada, visível só na chamada |
| `chamada.palco` | **vista alternativa** à grade, que vira uma coluna de cartões de 160 px (`:531-541`) | largura da tela; o MOD recebe o tamanho (§7.2) | aplicada |
| `pagina` | célula inteira; a imersiva **não cobre a faixa de voz nem as abas** | — | [inf] hoje cobre: é `fixed; inset: …; z-index: 30` sem área segura de baixo (`mods-superficies.css:142-146`). Conserto na tarefa 7 |
| `folha` | folha de baixo | até a altura do polegar, expansível | — |
| `dialogo` | tela cheia, ações fixas embaixo | — | — |
| `aviso` | canto de baixo, com a área segura (`celular.css:79-84`) | `min(420px, 100vw-32px)` | — |

**Uma contribuição para uma região que o celular não tem é recusada com motivo
nomeado** (`contribuicao` com `motivo: 'regiao-ausente'`), e não some calada.
A aba DIAGNÓSTICO conta. O MOD sabe onde está pelo `onde` e pelas `regioes` do
retrato, e pelo evento `lugar` quando o aparelho deita.

**Pontos ainda não medidos, que a F4 e a M3 no iPhone respondem:**

- o custo de capas animadas e de fundo por mídia num iPhone (o teto de memória
  de uma aba é muito menor, e passar dele encerra a aba);
- o som de MOD com a tela bloqueada: ele só sobrevive no caminho de saída da
  voz, e só dentro de uma sala de voz (MESA, item 14; D-52). E o MOD só roda
  quando o anfitrião o chama (§16), o que, com a página oculta, pode não
  acontecer. Até a medida, **a API 6 promete som de MOD só com a tela acesa**
  no celular;
- o arraste por toque na `tela`: um dedo sobre uma figura com `chave` arrasta,
  um dedo no vazio rola, pinça amplia dentro da tela, área de acerto mínima de
  22 px de raio, toque longo opcional por figura (MESA, item 15). **Entra no
  contrato**, porque só o produto vê o toque bruto; os números sobem da medida;
- o HEIC das Fotos e o GIF que chega parado (PERFIS, «No celular»): a ponte
  web converte, ou o MOD diz que não aceita.

## 12. Os limites

**Os limites congelam com o `v6.json`, e por isso esperam a M3** (spec-mestre,
F3). A medição de base da v0.15.1 está no ADR 0056 (§15). **Cada número vale
igual no desktop e no navegador**, porque o motor é o mesmo (§16). Os valores
de hoje, lidos no código:

| limite | hoje | onde [c] | na 6 |
|---|---|---|---|
| mensagem do MOD à janela | 12 KiB | `executor.rs:70`; `control.rs:2711`, `:2830`; `base.js:566` | **em aberto**: montagem parcial ou teto por superfície maior (MESA item 1, PERFIS R8) |
| contribuições por MOD | 128 | `mods-contribuicoes.js:91` | **em aberto**: por ponto e por alvo (cartão, avatar e capa de 64 pessoas dão 192, PERFIS B10, R2) |
| superfícies | 12 | `mods-superficies.js:204` | M3 |
| avisos | 4 | `mods-superficies.js:206` | fica |
| nós por superfície / fundura / campos / telas / mídias | 4096 / 16 / 128 / 4 / 24 | `mods-regiao.js:193` | M3 |
| bytes de mídia por superfície | 24 MiB declarados, **10 MiB na forma `midia`** | `mods-regiao.js:194`, `:2159-2161`, `:2226` | §13: a 6 diz o que é entregue |
| região | 512 nós, 8 de fundura, 10 MiB | `mods-regiao.js:33-65` | sai com `regiao` (§2), se sair |
| cartões | 64 pessoas, 64 nós, 6 de fundura, 64 mídias, 20 MiB somados | `mods-regiao.js:104-131` | troca por `pessoa.cartao` com `alvo` (R2) |
| som decodificado | 64 MiB por bolso | `mods-regiao.js:88`, `:148` | M3 e K5 |
| arquivos escolhidos | 4 de pé, 10 MiB cada | `main.rs:2438`, `:2444` | fica |
| imagem por volume | 10 MiB | `midia_de_mod.rs:59` | fica; a redução no Rust (R3) entra |
| pedidos em voo / fila da janela | 8 / 128 | `executor.rs:1505`; `base.js:535` | M3 |
| **novos**: capas animadas, quadros por capa, aviso do servidor | — | — | M3 (§4, §7.3) |

**Bolsos de mídia por bytes decodificados** (PERFIS R3, M6): hoje uma foto de
10 MiB gasta metade do bolso de todos os avatares do servidor. Na 6, a mídia de
servidor de um MOD é reduzida ao tamanho de uso antes de chegar à janela, os
bolsos contam bytes decodificados, e o MOD lê o teto para dizê-lo no seletor.

## 13. O que as versões publicadas tinham e não listavam, ou listavam e não entregavam

| o quê | onde [c] | na 6 |
|---|---|---|
| `pessoa.avatar` existe e nunca entrou num `vN.json` | `mods-contribuicoes.js:78`; ausente de `api/v5.json:131-163` | listado |
| o evento `contribuicao` é despachado e não listado | `base.js:1331-1336`; ausente de `api/v5.json:79-96` | listado |
| o `check-api` não confere pontos, tipos, formas, limites nem capacidades | `check_api.rs:321-330` | passa a conferir, nos dois sentidos (tarefa 2 do plano) |
| 16 dos 21 momentos da v1 nunca foram entregues | `despacho.rs:463-510` | só os 5 entregues |
| **os 13 verbos de `actions` e as 19 leituras de `reads` da v1 não são expostos à metade de servidor** | nenhum dos nomes aparece no código fora de `api/` | a v6 nomeia o que o runtime expõe (§8) |
| **os nomes de `own` da v1 não são os que o MOD escreve** (`guardar`, `lerArquivo` contra `dados`, `arquivos.ler`) | `mods/mod.rs:505-637` | idem |
| **a forma `midia` numa superfície conta contra o teto da região (10 MiB)**, e não contra os 24 MiB que a v5 promete | `mods-regiao.js:2159-2161`, `:2226` | a 6 diz o que é entregue, e um teste prova |
| um tipo de superfície desconhecido vira `dialogo` em silêncio | `mods-superficies.js:225` | recusa pelo nome |
| o retrato era o `Snapshot` inteiro, com `link_state` e os aparelhos | `base.js:2126-2133` | lista branca (§6) |
| o guarda de capacidades citado em `base.js:1963-1965` nunca existiu | `git log -S` | criado |
| **o exemplo `servidor-com-mod.rs` instala o pacote por identificador, e o servidor o procura por hash**: a metade de servidor recusa todo pedido com «No such file or directory» | `crates/seele-server/examples/servidor-com-mod.rs`; `RaizesDosMods::pacote_de`, `lib.rs:137-138` | **[medido]** na M3 (ADR 0056). Conserto fora desta frente: é do guia de quem escreve MOD |

## 14. O que a API 6 pede ao contrato de fio

O spec-mestre lista dois itens da F3 no contrato (§3): o 14
(`APIS_ACEITAS = [6]`) e o 15 (`ModAnunciado.api` e as APIs no `Hello`). O
rascunho acrescenta três candidatos, **a decidir com a F1 e a orquestradora**
antes de o contrato fechar. Pela D-fio-6, mensagem nova pode chegar numa 1.1
sem quebrar nada; o que está em jogo é se ela é **Servidor 1.0** ou **1.1 com
API 7**.

| candidato | quem pede | o que muda no fio | se ficar de fora da 1.0 |
|---|---|---|---|
| **o aviso da metade de servidor às janelas** (§7.3) | MESA 2, PERFIS R4, ESTILO §3 | uma mensagem servidor → cliente, `ModAviso { mod, canal?, carga }`, com teto | os três continuam em laço até a API 7; no celular, a MESA custa ~20 MB/h |
| **pedidos endereçados ao servidor** (§8) | PERFIS R5 | `ModRequest` com o canal opcional | o PERFIS não roda num servidor só de voz |
| **o tema como dado do servidor** (§9) | ESTILO §3 | o tema vai junto com a entrada na sessão | o tema continua chegando depois de um `snapshot` e de uma ida e volta |

**Recomendo que o primeiro entre na 1.0**: é o que mais custa a quem usa, e é
uma mensagem nova, sem campo em mensagem existente. Os outros dois mexem em
campo de mensagem que já existe, e a D-fio-6 diz que isso não pode chegar
depois: ou entram agora, ou o caminho deles é outra mensagem numa 1.1.

## 15. A medição (M3)

A M3 de base mede a v0.15.1 publicada, sem mudança nenhuma no binário, com 0,
1 e 3 MODs oficiais, nos cenários do ADR 0052:189-193: memória do app e da
webview, CPU, tempo de quadro e jitter da voz. **Os números e o roteiro estão
no ADR 0056.** A M3 com as regiões da 6 repete o mesmo roteiro, e os limites
do §12 saem dela, com o iPhone (F4, item 3) e o WebView2 pedidos à
orquestradora.

## 16. O executor do navegador: um contrato só, e um motor só

**O spike da F4 escolheu** (spec-mestre `00935dac`, §2 F4;
`spikes/mods-no-navegador/README.md` no branch `0.16/web`): **o mesmo
`rquickjs` 0.12.2 do desktop, compilado para `wasm32-wasip1`, num Worker
criado de dentro de um `iframe` de origem opaca**, com o prelúdio extraído do
`executor.rs` no build. O `iframe` sozinho reprovou: 8 pacotes STUN saem por
cima da CSP, o laço sem fim trava a página por 3 s, e a memória não tem teto.

**O que isso fixa nesta API:**

1. **Nada aqui depende de o código do MOD rodar fora de uma volta do
   anfitrião.** O prelúdio pressupõe isso nos dois executores: um evento
   (§7), um temporizador ou a resposta de um pedido só chegam ao MOD quando o
   anfitrião o chama. No celular, com a página oculta, o anfitrião pode não
   chamar: **a API 6 não promete que um MOD rode com a tela bloqueada** (§11,
   o som de MOD).
2. **Os limites valem igual nos dois executores**: memória, tempo de volta,
   contribuições, superfícies, o teto de 12 KiB por mensagem. No web, o teto de
   memória só existe com o `RustAllocator` (com o alocador padrão do QuickJS,
   um MOD alocou 128 MiB com teto de 8 MiB), e a F4 põe guarda. O §12 lista os
   números uma vez, para as duas plataformas.
3. **O custo do motor no web entra na M3 do celular**: 942 KB (361 KB com
   gzip), 14 a 59 ms para um MOD subir, de 5 a 7 vezes mais lento que o JIT,
   e a ida e volta entre a página e o MOD abaixo de 1 ms. A medida no iPhone
   é do dono.
4. **As capacidades ausentes por plataforma continuam** (§2): o que a ponte
   web não tiver some do `SeeleUI` lá.

**A superfície que ele expõe é a deste documento, e é a mesma do desktop**:

- o prelúdio inteiro de `executor.rs` (`SeeleMods.request`, `SeeleUI.*`, os
  quatro temporizadores, o `console` da 0.15.1), com `SeeleUI.retrato()` no
  lugar de `SeeleMods.snapshot()`;
- as capacidades **por plataforma** (§2): o que a ponte web não tiver fica
  ausente;
- o teto de 12 KiB por mensagem, conferido antes de postar, igual
  (`executor.rs:1504-1516` [c]), enquanto o §12 não mudar;
- nenhum armazenamento que sobreviva à sessão, nenhuma rede, nenhum acesso à
  origem da página (os critérios do spike, que o AB passou no Chromium 151 e
  no Safari do Simulador).

**A MESA não usa armazenamento nem rede** e roda igual nas duas candidatas
(MESA, item 18). O PERFIS e o ESTILO também não, pela leitura das revisões
[inf]. O alinhamento fino passa pela orquestradora.

## 17. O que as sessões de MOD pediram: o que entrou, o que não entrou e por quê

### PERFIS (R1 a R13)

| # | pedido | situação | por quê |
|---|---|---|---|
| R1 | `pessoa.capa` com legibilidade, halo, estados, movimento reduzido e custo | **entrou** (§4) | é a decisão do dono; os números de redução, quadros e capas animadas ficam para a M3 |
| R2 | cartão por pessoa, sem gastar o teto, citando a imagem da pessoa | **entrou** a citação (`daPessoa`); **em aberto** o cartão por pessoa e o teto por ponto e alvo | o destino de `cartoes` é decisão do dono com as sessões (§2) |
| R3 | bolsos por bytes decodificados, redução no Rust | **entrou** (§12) | o custo é o quadro decodificado, e não o arquivo |
| R4 | eventos da tela, retrato projetado, aviso do servidor | **entrou** (§6, §7); o aviso **depende do fio** (§14) | — |
| R5 | pedidos endereçados ao servidor | **em aberto** (§14) | muda campo de `ModRequest` |
| R6 | formulários no diálogo | **entrou** (§5) | — |
| R7 | `contribuicoes.atualizar(handle, conteudo)` | **entrou** | só fachada; evita o redesenho nativo entre revogar e registrar |
| R8 | montagem parcial nas superfícies | **em aberto** com o teto de 12 KiB (§12) | a MESA pede o mesmo (item 1); a forma (atualizar por chave ou montar em partes) é a primeira decisão de desenho da tarefa 7 |
| R9 | a falha dita a quem pode agir | **entrou**: `midia` com ponto e alvo (§7.1); a recusa `mensagem-grande` vai ao `seele.log` | — |
| R10 | o apelido verificado à vista | **entrou** no núcleo (§10) | D-nucleo-1 |
| R11 | presets de animação para moldura de retrato e capa; movimento reduzido em toda região | **entrou** a regra de movimento em toda região; **não entrou** o preset novo | a moldura é desenho do MOD, com o vocabulário de estilos que já existe; um preset novo é decisão de estética (`specs/07`), e não de API |
| R12 | `folha` para o perfil | **entrou** (§5) | — |
| R13 | capacidades verdadeiras por plataforma | **entrou** (§2) | — |

### MESA (itens 1 a 18)

| # | pedido | situação | por quê |
|---|---|---|---|
| 1 | montar sem esbarrar em 12 KiB | **em aberto** (§12), com o R8 do PERFIS | — |
| 2 | o servidor avisar as janelas | **entrou**, dependendo do fio (§7.3, §14) | — |
| 3 | eventos da tela com o retrato escrito no contrato | **entrou** (§6, §7.2) | — |
| 4 | `repetir`, som que não morre com a página, início pela posição do servidor | **em aberto** (§18) | é o m-4 da pendência 49, do dono com as sessões |
| 5 | `chamada.palco`: contexto, gramática, de quem, teto | **entrou** (§5): contexto pela sala, gramática inteira, por pessoa; o teto com o item 1 | — |
| 6 | `folha` ao lado da mesa | **entrou** (§5) | — |
| 7 | formulários com validação e erro por campo | **entrou** (§5) | — |
| 8 | a `tela` de jogo: `cancelou`, grade, deslocar e ampliar | **entrou** `cancelou` (§7.1) e o gesto por toque (§11); **em aberto** a primitiva de grade e a tela maior que 1024 px | a grade é forma nova da gramática, e cada forma é decisão de API registrada (ADR 0052); entra se a M3 mostrar que 122 figuras por grade custam |
| 9 | o orçamento do quintal à vista | **entrou** (§8) | — |
| 10 | `canal.cabecalho`, `sala.acoes`; `pessoa.detalhes` montado por quem vê | **entrou** o uso dos dois pontos; **em aberto** a montagem por quem vê | a montagem por quem vê é uma forma de contribuição que ainda não existe; peço o desenho à MESA no porte |
| 11 | o diagnóstico ler a mídia das páginas e das camadas | **entrou** (é o K2 da F5, estendido às camadas novas) | — |
| 12 | `chamada.palco` no celular como vista alternativa | **entrou** (§11) | — |
| 13 | a `folha` como folha de baixo; o teclado | **entrou** (§5, §11) | — |
| 14 | a trilha com a tela bloqueada | **não entrou** como promessa; a 6 promete som de MOD só com a tela acesa no celular | não medido; volta à mesa depois da medida da F4 |
| 15 | o arraste por toque | **entrou** (§11), com os números da medida | — |
| 16 | onde o MOD está; recusa nomeada para região ausente | **entrou** (§6, §11) | — |
| 17 | a consulta de 2 s no celular | resolvida pelo item 2 | — |
| 18 | o executor no navegador com a mesma API | **entrou** (§16) | — |
| M14 | o gesto cancelado move a peça | **entrou** (`cancelou`) | — |

### ESTILO (seções 1 a 8)

| § | pedido | situação | por quê |
|---|---|---|---|
| 1 | tema que cobre o que o produto pinta | **entrou** o contraste em todos os pares e os tons derivados; **em aberto** `modo`, tipo, escala, densidade completa e fundo por mídia (§9) | o fundo por mídia espera a medida no iPhone |
| 2 | tokens por região; `folha` e `dialogo` herdam o tema; referência a token | **entrou** a herança e a referência; **em aberto** os tokens por região | recomendo que entrem |
| 3 | tema como dado do servidor; aviso do servidor; `servidor.aparencia` como porta; prévia do produto | **entrou** a porta e a prévia; **em aberto** o tema como dado (fio, §14); o aviso depende do fio | — |
| 4 | `erro` em `cor` e `escolha`; confirmação destrutiva; `emProgresso`; sujo derivado | **entrou** (§5) | — |
| 5 | o que fica fora; o contraste do núcleo com guarda | **entrou** (§9, §10) | é o achado do spec-mestre §8 |
| 6 | o que falta na spec | **entrou** como emendas à spec de 23/09 (tarefa 13 do plano) | — |
| 7 | notas para o `v6.json` | **entrou** (`pessoa.avatar`, o `tema` que diz onde vale e quem o tira, o retrato) | — |
| 8 | o celular: o mesmo tema, as três exceções do produto, o que a casca não deixa tocar, a edição pelo celular | **entrou** (§9, §10, §11) | D-estilo-1 |
| 8 | `theme-color` derivado do `fundo` no web app | **não entrou na API**: é do produto, e o MOD não toca nele | fica como pedido à F4 |

### F5 (seção «Para a F3» do `lotes.md`)

| item | situação |
|---|---|
| K1-m3: o MOD recebe o motivo, e não a frase de tela, no erro do `atenderOMod` | **entrou**: na 6 o `erro` que o MOD recebe é um nome de recusa, de lista fechada, e a frase fica na tela |
| P51-28: o guarda do vetor de referência | **entrou** na tarefa 13: reescrito contra o prelúdio da 6 |
| P49-10: a frase dos cartões da API 3 | sai com a capacidade `cartoes`, se ela sair |
| K2: `midia`/`falhou` quando o retrato não decodifica | **entrou** (§7.1) |
| K4 e K5: o som grande demais e o som de contribuição recusado no tocar | **entrou** o texto público, quando os lotes chegarem |
| o contrato de `tocando` (P49-6, `563bc37`) | **entrou** no texto da 6, como está no `api/README.md`; `repetir` está em aberto (§18) |
| `capture`/`playback` no `Snapshot` | **entrou**: ficam fora da lista branca (§6) |

## 18. Em aberto

**Para a M3:** os limites do §12, os novos (capas animadas, quadros, aviso do
servidor) e os do celular.

**Para o dono e as sessões de MOD:**

1. `pagina` e `painel` contra `folha` (§5). Recomendo que a `folha` substitua o
   `painel`.
2. `regiao`, `cartoes` e `tema` (§2). Recomendo que `regiao` e `cartoes` saiam
   e que `tema` fique, ampliado e com `servidor.aparencia` como porta.
3. **O m-4 da pendência 49: `repetir`, ou o contrato de `tocando`.** Hoje um
   som que terminou toca de novo quando um redesenho o declara `tocando: true`
   (`mods-regiao.js:2503-2510` [c]), e é isso que repete a trilha da MESA. Se o
   contrato for «`tocando` vale na mudança» sem `repetir`, a trilha de 30 s
   cala no primeiro conserto do laço de 2 s (MESA M7). **Recomendo `repetir:
   true` por contrato** (`AudioBufferSourceNode.loop`), e manter a exceção do
   som que terminou como está até lá.
4. Os três candidatos ao fio (§14), com a F1.
5. **Os membros fora do ar no retrato** (§6). Recomendo que fiquem de fora na 6.

**Para a orquestradora:** o WebView2 (Windows) e o iPhone na M3; a forma de
rodar o MOD no navegador (F4), contra o §16.
