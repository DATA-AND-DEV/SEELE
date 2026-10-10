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

### Decisões do dono (08/10/2026) sobre este rascunho

Tomadas depois da primeira versão, todas como o rascunho recomendava. Elas
mudam o texto abaixo, e cada seção já as segue.

| pergunta | resposta |
|---|---|
| `pagina` e `painel` contra `folha` (§5) | **a `folha` substitui o `painel`**. Ficam `pagina`, `dialogo`, `aviso` e `folha` |
| as capacidades `regiao`, `cartoes` e `tema` (§2) | **`regiao` e `cartoes` saem. O `tema` fica**, ampliado, com `servidor.aparencia` como porta |
| o m-4 da pendência 49 (§18) | **`repetir: true` por contrato** (`AudioBufferSourceNode.loop`). A exceção do som que terminou fica como está até lá |
| os membros fora do ar no retrato (§6) | **ficam de fora** |
| os três candidatos ao fio (§14) | **o aviso do servidor e o tema como dado vão para o contrato da 1.0**, pela F1; os pedidos de escopo de servidor **não mudam o fio**, porque já existem (§8) |

### Decisões do dono (09/10/2026)

| pergunta | resposta |
|---|---|
| dois MODs com a porta do tema (§9) | **um tema por servidor**, o escolhido por quem administra (D-api-5) |
| o teto de contribuições (§12) | **o modelo está decidido** (os pontos por alvo fora do agregado); **os números** (256 alvos por ponto, 128 no agregado) **saem da M3** |
| o que não entrar na 6 | **é candidato da API 7** (Servidor 1.1), e fica escrito no ADR 0056 |

---

## 0. O que muda, numa página

1. **Só a API 6 roda no app 0.16.** Quem precisa de um MOD das APIs 3, 4 ou 5
   usa o Servidor 0.15 pelo launcher. **Servidor 1.0 = protocolo 1.0 + API 6.**
2. **A janela inteira vira mapa de regiões nomeadas**, com dois modos
   (`adicionar` e `substituir`). Os pontos de hoje continuam com o mesmo
   nome e passam a morar dentro das regiões; `servidor.aparencia` vira a porta
   de quem vê sobre o tema.
3. **`pessoa.capa`**: a capa de uma pessoa, no cartão da chamada, no fundo da
   barra do operador e nos dois cabeçalhos de perfil.
4. **Camadas**: `chamada.palco` (sobre a chamada), `folha` (coluna que
   empilha), `dialogo` com formulários declarados, e o `aviso` de sempre.
5. **O MOD deixa de ler o `Snapshot` inteiro.** Ele lê um **retrato projetado
   por lista branca** e assina **eventos da tela**. O veredito de conexão
   segura, os aparelhos da máquina e a telemetria saem do alcance dele.
6. **O tema é dado do servidor**: a metade de servidor o declara
   (`mundo.tema`), o servidor o guarda e o manda logo depois do `Session`, e o
   produto o aplica e diz ao MOD o que fez (§9). Um tema por servidor. A parte
   do servidor está feita (tarefas 17a a 17c); a da casca espera o K4.
7. **A metade de servidor do MOD avisa as janelas** de que um dado dela mudou.
   É o que tira a consulta em laço dos três MODs oficiais (2 s na MESA, 4 s no
   PERFIS e no ESTILO).
8. **Toda região e toda camada diz o que faz no celular** (D-web-1): se existe
   na casca de uma mão, com que tamanho, e o que acontece com uma contribuição
   para uma região que lá não existe.
9. **A v6 promete só o que é entregue**, e é escrita por inteiro, sem
   `extends`. O que as versões 1 a 5 prometiam e nunca entregaram não entra
   (§13).
10. **O núcleo de confiança vira contrato testado**, e passa a incluir o
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
| `superficies` | sim | `pagina`, `dialogo`, `aviso` e `folha`; o `painel` sai (decisão de 08/10, §5) |
| `contribuicoes` | sim | as regiões e os pontos (§3) |
| `estilos`, `classes` | sim | sem mudança de contrato |
| `volume` | **sim** | ADR 0048. O PERFIS (R1, R3), a MESA (M11, item 16) e o ESTILO (§1, fundo por mídia) dependem dela. Confirmado à F5 em 08/10: o lote E (P51-11) tem objeto |
| `arquivo` | sim | o seletor mediado e `pedaco`/`soltar` |
| `tema` | **sim, ampliado** (decisão de 08/10) | §9, com `servidor.aparencia` como porta |
| `regiao` | **sai** (decisão de 08/10); **ausente** do `SeeleUI` da 6, como `superficies` do da 3, e não montado para a janela recusar depois [c, tarefa 3] | a faixa de 240 px é o U01–U03 da auditoria de 20/09 (ADR 0052). Nenhum oficial a pinta: os três calculam `aRegiao` e jogam fora (PERFIS B4, MESA B3, ESTILO A19) |
| `cartoes` | **sai** (decisão de 08/10), trocada por `pessoa.cartao` com `alvo`; ausente do `SeeleUI` da 6 [c, tarefa 3] | o conjunto inteiro vai numa mensagem de 12 KiB e congela a lista com 14 pessoas (PERFIS A1, R2) |
| `retrato` | **sim, nova** (09/10, na tarefa 4) | `SeeleUI.retrato()`, projetado por lista branca (§6), **no lugar** de `SeeleMods.snapshot`: quem tem `retrato` não tem o `snapshot` inteiro, nem no prelúdio nem no roteador (`SUBSTITUIDA_PELA_CAPACIDADE`). Uma capacidade, e não um teste de versão, porque é pelas capacidades que o prelúdio monta o `SeeleUI` [c] |
| `eventos-da-tela` | sim, nova | §7 |
| `aviso-do-servidor` | sim, nova (metade de servidor) | §7.3. No contrato de fio da 1.0, pela F1 (§14) |

**Capacidade por plataforma, e não só por versão.** Hoje a ausência é por
versão: um pacote de API 3 recebe um `SeeleUI` sem `superficies`
(`comCapacidade`, `executor.rs:1643` [c]). Na 6, o executor do celular omite
também o que a ponte web não tem (PERFIS R13). Se ela não tiver
`enviar_imagem_mod`, o `SeeleUI.enviar` não existe lá, e o MOD mostra o editor
sem os controles de imagem, com uma frase. **Ausente, e não recusado depois de
a pessoa escolher a foto.**

**O guarda que nunca existiu.** O comentário de `base.js:1963-1965` [c] cita
`a_casca_confere_as_mesmas_capacidades_que_o_nucleo`. O teste não existia: o
`git log -S` só achava o comentário (`26ad0c2`). A tarefa 2 do plano o criou
(`0b012c1`, em `apps/seele-app/tests/frontend.rs`): `CAPACIDADES_POR_API` na
casca e `capacidades_da_api` no Rust são comparados por inteiro, e as três
reversões provadas reprovam. [c]

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

**Os pontos de hoje ficam com o mesmo nome**, menos `servidor.aparencia`, que
deixa de ser registrado pela janela e passa a ser a porta de quem vê sobre o
tema declarado pelo servidor (§9).

**`listarNaBarra` continua** (pergunta do ESTILO, B6): a entrada que um MOD
registra em `servidor.navegacao` pode pedir `listarNaBarra: false`, e então
ela não aparece na lista de quem não tem uso para ela, sem mudar permissão
nenhuma (`api/v4.json`, `contribuicoes.listarNaBarra`). No celular a entrada
mora na aba SERVIDOR (§11), e o `false` vale lá do mesmo jeito. `pessoa.avatar` existe desde `14d9c30` (22/09) e
saiu na 0.15.0, mas **nunca entrou num `vN.json`** (§13). A v6 o lista.

**Uma substituição de região inteira não leva os pontos de dentro.** Quem
substitui `pessoas.lista` desenha a lista, e o produto continua aplicando
`pessoa.avatar` e `pessoa.capa` de quem os registrou, e continua desenhando a
moderação e o apelido verificado de cada linha (§10). [inf] Isto é o que
mantém duas garantias juntas: a de identidade (o menu age pela chave) e a de
leitura (D-nucleo-1). **A forma exata de uma região substituída que ainda
carrega os itens do núcleo é a primeira coisa a desenhar na tarefa 5 do
plano** — desenhada abaixo, para revisão.

### A região substituída que ainda carrega o núcleo (tarefa 5, Step 1; aprovado em 09/10)

**Substituir uma região é desenhar o miolo de cada linha, e não a moldura.** A
moldura é do produto: o `<ul>` e cada `<li>` (o leitor de tela conta «3 de
12»), a faixa do núcleo de cada linha, e o que a linha diz sobre a verdade da
conversa. O MOD desenha o que está entre eles. Medido no código de hoje
(`linhaSubstituida`, `tela-sessao.js` [c]): com `pessoa.cartao` substituído, o
apelido real e o menu de moderação descem para um `<details>` recolhido («⋯»).
O menu continua alcançável, mas **o apelido não fica à vista**, que é o que a
D-nucleo-1 pede. A forma abaixo vale também para `pessoa.cartao`, e conserta
isso.

**`pessoas.lista`, substituída.** O MOD toma **a lista**: a ordem, os grupos e
os títulos deles. A linha de cada pessoa continua vindo de `pessoa.cartao` —
dele ou de quem for o escolhido para esse ponto —, e é por isso que a
declaração cabe no fio com 250 pessoas (os identificadores, ~1 KiB, e não 250
cartões):

```js
SeeleUI.contribuicoes.registrar({
  ponto: 'pessoas.lista', modo: 'substituir',
  conteudo: { grupos: [{ titulo: 'MESA 1', pessoas: [12, 7] }, { titulo: 'PLATEIA', pessoas: [3] }] },
});
```

- **As salas de voz continuam seções do produto** (decisão da orquestradora,
  09/10). Quem está na sala de quem ouve você é verdade de confiança, do mesmo
  tipo do apelido verificado, e um MOD não a reorganiza: os grupos do MOD
  valem **dentro de cada sala** (e dentro de FORA DE SALA e de NO SERVIDOR,
  FORA DAS SALAS), como subtítulos, na ordem dele. Um MOD que põe pessoas de
  duas salas no mesmo grupo vê cada uma na sua sala. Grupos que cruzam salas
  ficam para a API 7, se alguém pedir. O FORA DO AR é seção do produto, no
  fim, e o MOD nem sabe dele: o retrato da 6 não leva os membros fora do ar
  (§6).
- **Ninguém some da lista.** Quem está na sala e não está em grupo nenhum do
  MOD fica no fim da sala, sob um subtítulo do produto (OUTRAS PESSOAS). Um identificador de quem não está é ignorado, e um que aparece em
  dois grupos fica no primeiro; os dois contam na aba DIAGNÓSTICO. Nenhum dos
  dois é recusado: a lista muda mais depressa do que o MOD a reescreve.
- **A linha tem três faixas**, numa grade do produto: `[núcleo] [miolo] [⋯]`.
  - **o núcleo**, à esquerda [c, tarefa 5, também sob `pessoa.cartao`]: o
    apelido verificado, lido do retrato **pela
    chave da linha**, e nunca do que o MOD escreveu. Quando esta sessão tem
    verbo sobre a pessoa, ele **é** o botão de moderação, o mesmo elemento da
    linha nativa (`.pessoa-nome-porta`): a moderação sai do «⋯» e fica a um
    clique. O halo e a pastilha de quem fala também são dele. **Lê o `texto`
    e o `painel` do tema** (decisão da orquestradora, 09/10), que os pares já
    garantem a 4,5:1: o que protege o apelido é a estrutura — o lugar
    reservado à esquerda, o miolo fechado ao lado, o nome lido da chave —, e
    não a cor (§10, «O que protege cada item»). O apelido longo é cortado com
    reticências, com o nome inteiro no `title` e no nome acessível;
  - **o miolo**: o cartão de `pessoa.cartao` quando ele tem conteúdo, e senão o
    resto da linha nativa (o número e a barra de sinal, o microfone). A capa e
    o avatar entram aqui por citação (`fundoDeMidia: { daPessoa: 'capa' }`,
    `retrato({ daPessoa: 'avatar' })`, §4), com o carregamento do produto. O
    miolo é uma caixa com `overflow: hidden` e `contain: paint`: nada que o MOD
    desenhe — posição absoluta, transformação, margem negativa — sai dela, e a
    faixa do núcleo é **irmã** dele, nunca filha;
  - **o «⋯»**, à direita: o diagnóstico nativo, `pessoa.detalhes` e
    `pessoa.acoes`, como hoje.
- **Os pontos de dentro ficam com os provedores deles.** Quem substitui a lista
  não fica dono de `pessoa.cartao`, `pessoa.avatar` nem `pessoa.capa`: um MOD
  pode agrupar a lista e outro desenhar as linhas, e a escolha de quem
  administra vale ponto a ponto, como hoje.

**`servidor.navegacao`, substituída** (tarefa 5; a mesma regra, aplicada por
mim e levada à orquestradora): o MOD toma **a ordem e os grupos de cada
lista**, salas e canais, separadamente:

```js
SeeleUI.contribuicoes.registrar({
  ponto: 'servidor.navegacao', modo: 'substituir',
  conteudo: {
    salas: { grupos: [{ titulo: 'MESA 1', ids: [1] }] },
    canais: { grupos: [{ titulo: 'MESA 1', ids: [2] }] },
  },
});
```

A linha de cada sala e de cada canal continua do produto — ENTRAR, renomear,
apagar, e os `canal.item` e `sala.acoes` dos provedores deles —, e os `+` de
criar ficam nos cabeçalhos do produto. Quem não está em grupo vai para OUTRAS
SALAS / OUTROS CANAIS; um grupo sem ninguém presente não aparece; outra forma
de declaração é recusada pelo nome no registro. Uma substituição não vira
entrada «DOS MODS», que continuam sendo os `adicionar`. **Grupos mistos** —
uma sala e um canal no mesmo grupo — **ficam para a API 7**: as duas listas
têm verbos diferentes (entrar numa sala, abrir um canal), e misturá-las é
desenho de tela, e não só ordem. [c, `regioes-do-produto.cjs`]

**`conversa.mensagem`, substituída** (por mensagem, `alvo` sendo a mensagem).
O MOD desenha **o corpo**. O produto mantém, na ordem: a calha do avatar (com
`pessoa.avatar`), a cabeça (o apelido verificado do autor, lido pela chave, a
hora, «editada» e o REMOVER), o miolo do MOD no lugar de `.corpo`, as
contribuições `adicionar`, o bloco do anexo (ADR 0027, com o consentimento das
prévias) e o selo do envio (pendente ou recusada). O miolo tem a mesma caixa
fechada da lista.

- **O agrupamento passa a ser pela chave do autor** (`author`), e a cabeça
  aparece na primeira mensagem de cada grupo. Hoje ele é pelo apelido
  (`anterior.author_nickname === mensagem.author_nickname`,
  `tela-sessao.js:1605` [c]): se o servidor admitir dois apelidos iguais, a
  mensagem da segunda pessoa sai sem cabeça, debaixo da primeira — e com o
  corpo desenhado por um MOD, a cabeça é a única autoria à vista. A troca é
  desta tarefa, com o guarda; um nome que o MOD escreva no corpo nunca ocupa o
  lugar dela.
- **A busca vence.** Enquanto a busca marca uma mensagem substituída, ela volta
  ao corpo nativo: o realce é o que a pessoa procura, e o produto não marca
  dentro do desenho de um MOD. Saindo da busca, o miolo volta.
- **Uma mensagem pendente não recebe o desenho de um MOD** [c, tarefa 5]: ela
  tem `id` zero (o servidor ainda não a gravou), todas as pendentes dividiriam
  esse alvo, e uma substituição sem alvo vale para todas as mensagens. O texto
  dela e o selo do envio são do produto até ela ser gravada. [c, toda esta
  seção de `conversa.mensagem`: `regioes-do-produto.cjs`]

**Os guardas** (Steps 2 a 5, e o guarda 11):

1. o apelido da faixa do núcleo é o do retrato: um cartão que escreve «alice»
   na linha de bob deixa «bob» no núcleo;
2. um miolo com um elemento em posição absoluta por cima do núcleo:
   `elementFromPoint` no centro do apelido devolve o elemento do produto;
3. um grupo que omite uma pessoa presente: ela aparece em OUTRAS PESSOAS;
4. o botão de moderação é alcançável sem abrir o «⋯» (Tab e clique);
5. uma mensagem substituída mantém a cabeça, o anexo e o selo; com a busca
   marcando, o corpo nativo e o realce voltam;
6. duas mensagens seguidas de pessoas diferentes com o mesmo apelido: cada uma
   tem a sua cabeça.

**As três escolhas, decididas pela orquestradora em 09/10:**

- **o apelido nas linhas lê as cores do tema**, e não a placa do produto: uma
  placa escura em cada linha de um tema claro pesaria a lista inteira, e um
  MOD que desenhe um apelido falso no miolo o desenha **ao lado** do
  verdadeiro, e não no lugar dele (§10);
- **o apelido sai do «⋯» também sob `pessoa.cartao`**: é a D-nucleo-1. A linha
  de quem usa o PERFIS muda, e é para mudar (§17, PERFIS R10). Guarda: com
  `pessoa.cartao` substituído, o apelido está à vista, fora de todo
  `<details>`;
- **o MOD ordena e agrupa, e não esconde** quem está presente.

E o achado do agrupamento das mensagens: **pela chave, nesta tarefa, com
guarda** (o 6 acima), e a reversão — agrupar pelo apelido — reprova.

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
| a identidade de quem usa na barra do operador (`#operador-quem`, com `snapshot.me`) — **só ela**, e não a barra inteira: o MUDO, o fone e o SAIR moram na barra, e nunca ficam sobre a capa (§10) [c, tarefa 6] | sim | **não há barra na vista**: o `operador.barra` mora em AJUSTES (§11), e a capa do operador pinta o cabeçalho SEU PERFIL logo acima dele (a linha de baixo); a faixa de voz é do núcleo e não recebe capa |
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

- **o texto do produto fica em placas opacas** da cor do fundo que ele teria
  sem a capa — o negro do painel, e o laranja de carga de quem fala —, ou num
  véu opaco onde há texto. A capa aparece no resto [c, tarefa 6];
- **todo texto do produto sobre a capa mantém pelo menos o contraste que tem
  sem capa**, conferido contra um quadro branco e um quadro preto, com guarda
  [c, `capa.cjs`: cada elemento com texto do cartão e da identidade, calado e
  falando; sem a placa, o nome cai de 15,99:1 para 1,28:1 sobre a capa branca];
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
  entregá-la à janela, e limita quadros e duração (proposta: 960 × 320, 60
  quadros, 10 s) [c, tarefa 6, Step 5a: a janela nomeia o uso `"capa"`, de
  lista fechada no Rust, e nunca um tamanho; `reducao.rs`];
- o bolso da capa conta **bytes decodificados** [c, Step 5b: 128 MiB por MOD,
  a partida, separado do bolso dos avatares, que segue contando o arquivo];
- **no máximo N capas animadas ao mesmo tempo** (proposta: 6, contando a do
  operador), escolhidas pela vista: as visíveis, e entre elas quem falou por
  último. O teto não depende do tamanho da sala (até 250 pessoas,
  `control.rs:77` [c]). [c, Step 5c; as outras mostram o quadro parado]

## 5. Camadas e superfícies

| camada | o que é | modos ou tipo | celular |
|---|---|---|---|
| `chamada.palco` | sobre a chamada, com a grade de vídeo do produto por baixo (`#chamada-grade`, `index.html:1407` [c]) | ponto de perfil `superficie`, `substituir` e `adicionar` | **vista alternativa** à grade (MESA · CHAMADA), com quem fala numa tira no alto |
| `dialogo` | modal; a saída é do produto (`mods-superficies.js:190` [c]) | superfície | **tela cheia acima da faixa de voz e das abas**, nunca por cima delas, com as ações fixas embaixo (§10) |
| `folha` (nova) | coluna lateral que empilha, para edição e detalhes; parente do `painel` (`#palco-de-paineis`, `index.html:1702` [c]) | superfície | **folha de baixo**: sobe de cima da faixa de voz até onde o polegar alcança e se expande até o alto, sem cobrir a faixa nem as abas |
| `aviso` | fila de quatro, como hoje (`mods-superficies.js:206` [c]) | superfície | canto de baixo, acima da faixa e das abas, com a área segura |
| `pagina` | como na API 4 | superfície | célula inteira (§11), acima da faixa e das abas |

### `chamada.palco`

- **Contexto.** A chamada é de uma sala de voz, e um pedido do MOD é por canal
  (`SeeleMods.request(id, canal, valor)`, `executor.rs:1615` [c]).
  **`contexto.sala` é preenchido pelo servidor em todo pedido**, e não só nos
  feitos do palco (pergunta da MESA, 09/10): é a sala de voz em que a pessoa
  está naquele instante, segundo a presença que o servidor mantém, ou `null`
  fora de sala. Por construção, a janela não o manda e não o forja, e não há
  o que conferir. Não é dado novo para o MOD: a metade de servidor já recebe
  `PersonJoined` com a sala (§7.4). O retrato também diz a sala ocupada (§6).
  Não amarro sala a canal na API: é o MOD que decide (MESA, item 5a).
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

### A mídia do servidor, em todo lugar, pelo mesmo caminho

**Toda mídia que vem da metade de servidor aceita `transporte: 'volume'`**
(pergunta da MESA, 09/10): a forma `midia`, o `fundoDeMidia` de uma caixa, o
`retrato`, a `pessoa.capa` e **o `fundo` da `tela`** (o mapa sob as peças). É o
mesmo `doServidor: { canal, pedido, campo }` da v5, com `pedido.transporte =
'volume'` levando a leitura ao fluxo do ADR 0048, e não ao controle. A v5 só o
dizia da mídia em geral (`api/v5.json`, `volume.leitura`); a 6 o diz de cada
lugar, e uma bancada prova o `fundo` da `tela` (tarefa 7).

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
- **`sujo` derivado**: o produto compara o valor atual de cada campo com o
  inicial, e a pergunta ao fechar não depende de o MOD chamar `suja`. **O MOD
  pode declarar os dois** (pedido do PERFIS, 09/10): `{ forma: 'campo', chave,
  valor: <o rascunho guardado>, inicial: <o gravado> }`. Assim o editor que
  reabre com um rascunho já nasce sujo, e voltar ao gravado o limpa. Sem
  `inicial`, o inicial é o `valor` da montagem;
- **confirmação declarada para ação destrutiva**, com as palavras do produto:
  `{ confirmar: 'isto muda a aparência para todo mundo neste servidor' }`
  (ESTILO A13). **Ela se declara em qualquer `botao`**, e não só no primário
  (pergunta do ESTILO, B7): RESTAURAR PADRÃO é um terceiro botão, destrutivo,
  ao lado do primário e da saída. O produto mostra a confirmação com as
  palavras dele por cima do diálogo, e o evento `botao` só chega ao MOD
  depois do sim;
- **um erro do servidor que fala de dois campos** vai a
  `formulario.erros({ text: motivo, background: motivo })`: o produto marca
  cada campo pela chave. O que a metade de servidor responde é protocolo do
  próprio MOD (o produto só repassa o `ModReply`);
- **`emProgresso` no primário trava o segundo clique** (ESTILO A6);
- **tela cheia com ações fixas** quando o produto decide (celular, ou
  `largo: true` numa janela estreita), com `abas` cabendo em 390 px
  (D-estilo-1). No celular, «tela cheia» é a tela **acima da faixa de voz e
  das abas** (§10).

### O que fica como está

O `aviso` (fila de quatro, 3,5 s e 10 s) e as garantias de saída do ADR 0052.

### `pagina` e `painel` contra `folha`: decidido

**A `folha` substitui o `painel`** (decisão do dono de 08/10, como o rascunho
recomendava). A spec de 23/09 já desenhava a `folha` como «parente do painel»,
o porte é obrigatório de qualquer jeito (API-6-só), e um tipo a menos é uma
superfície a menos para revisar e para descrever no celular. A `pagina` fica,
porque a MESA imersiva e o diretório do PERFIS precisam da célula da conversa.
O que os oficiais montam hoje num `painel` vai para a `folha` no porte.

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

**Todo campo novo do `Snapshot` nasce fora do retrato** (achado da revisão do
I-b da F5, 09/10). O I-b acrescenta `telemetry.microfone_entrega_silencio_digital`,
um sim ou não que conta o estado de uma permissão de privacidade do sistema.
O MOD não tem o que fazer com ele, e na API 5 ele chegaria: lá o MOD lê o
`Snapshot` inteiro. Na 6, o retrato é copiado campo a campo, em todos os
níveis, e **a lista permitida da `telemetry` é vazia**: nenhum campo dela chega
ao MOD. O guarda prova isso pela forma, e não por uma lista do que se proíbe: as
chaves do retrato, em cada nível (o topo, `servidor`, cada pessoa, cada sala,
cada canal), são **exatamente** as da tabela acima, e um `Snapshot` com um campo
desconhecido em cada nível (o do I-b inclusive) sai sem nenhum deles. Um campo
que alguém quiser dar ao MOD entra pela tabela, e não por esquecimento; depois
do congelamento, tirar um campo quebra a API (tarefa 4, passo 4b).

**Onde o campo chega hoje** [c, conferido em 09/10]: em nenhum lugar. Ele só
existe no ramo `0.16/lotes-i-b`, ainda fora da `versao/0.16`, e não existe na
v0.15.1 publicada (`54f542f`). Quando o I-b entrar, ele chega aos MODs da API 5
**na build da 0.16 antes do corte**, pelo `snapshot` sem filtro
(`base.js:2335`, `crates/seele-motor/src/preludio.rs:277`). Nenhum MOD
instalado por alguém o recebe: o app 0.16 só sai depois do corte, com
`APIS_ACEITAS = [6]` e o retrato. A linha 0.15 não tem o campo; ele só chegaria
lá se o I-b fosse levado a ela, e aí o mesmo `snapshot` sem filtro o entregaria.

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
| `membros` (o FORA DO AR: `Membros`, `MembroNovo`, `MembroBanido`) | **fica de fora na 6** (abaixo; decidido em 08/10) |
| **toque, chamada, código, agenda** | nunca (Parte II, «Onde as duas partes se tocam»). O guarda 12 prova |

**`reads` lista cada campo**, e o `check-api` passa a cobrar que cada um tenha
quem o projete. Um retrato que cresce é uma API somada.

### Os membros fora do ar

A F1 pôs no protocolo o FORA DO AR (`Membros`, `MembroNovo` e `MembroBanido`,
ordinais 42 a 44, com `id` e `nickname`, sem papéis e sem horário; D-fio-4).
**Decidido em 08/10, com a recomendação desta frente: o retrato da 6 não os
leva, e leva só quem está presente.** Os motivos:

- **A assimetria do congelamento.** Acrescentar `membros` depois é uma API
  somada (a 7), que não quebra nada. Tirar depois é impossível: o nome congela
  com o `v6.json`. Na dúvida, a porta que se pode abrir depois fica fechada.
- **Nenhuma sessão de MOD pediu os ausentes.** O PERFIS pede `me` e «as pessoas
  do servidor, com `id` e apelido» (R4) e a MESA pede «quem está no servidor»
  (item 3), os dois lendo `presentes` hoje.
- **Os casos que pediriam têm o dado do próprio MOD, ou não pedem.** O
  diretório do PERFIS **não** lista perfis de quem está ausente: os ids vêm de
  quem está presente (correção do PERFIS, 09/10, no plano do porte dele). A
  MESA, para dar uma ficha a quem joga e não está,
  tem a lista de quem já entrou na campanha (a metade de servidor recebe
  `PersonJoined` com o apelido, §7.4).
- **O MOD não é «todos».** A D-fio-4 diz que todos os presentes veem a lista,
  na tela do produto. Um MOD é código de terceiro, com rede de saída na metade
  de servidor (`mundo.buscar`). O que ele ganharia é **o rol de membros do
  servidor**, inclusive de quem não entra desde que o MOD foi ligado, e ele
  poderia mandá-lo para fora. O `specs/00-visao-geral.md` chama a presença
  publicada de defeito do formato que o SEELE recusa.

**Se um dia eles entrarem (numa API somada)**, a forma é `membros[]` com `id` e
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
  '12', estado: 'falhou', porque }`), para o MOD dizer a quem enviou que a capa
  não vai aparecer (PERFIS M5, R9). [c, tarefa 6, para o avatar e a capa; o
  `estado` é o `'falhou'` que o avatar já mandava, e o `alvo` vai como texto,
  como os alvos das contribuições] E o retrato que não decodifica manda
  `midia`/`falhou` (nota do K2 da F5).
- **`traco`** ganha a fase `cancelou`. Hoje `pointercancel` chega como
  `terminou` (`mods-regiao.js:1898` [c]) e um gesto cancelado move a peça
  (MESA M14). No celular é comum.
- **`contribuicao`** ganha o motivo `regiao-ausente` (§11), e o motivo
  **`orcamento`**: uma declaração que passa do teto do ponto é montada até o
  teto, e o MOD recebe `{ nome: 'contribuicao', ponto, destino, recusados,
  motivo: 'orcamento', perfil, teto, tamanho }` — o orçamento do ponto, quantos
  nós cabem nele e quantos a declaração tem [c, tarefa 5]. Até a tarefa 5 ele
  recebia só `recusados`, que não diz a quem escreveu quanto cabe nem quanto
  pediu. As recusas por forma (uma forma que o ponto não aceita) continuam sem
  `motivo`.

**Os nomes das recusas que o MOD recebe** (o K1-m3 da F5: o `erro` que chega ao
MOD é um nome de lista fechada, e a frase fica na tela). No envio e na leitura
de imagens por `volume`, a lista, com os nomes que o fio e o app já usam
(`VolumeRefusal`, `crates/seele-proto/src/volume.rs:70-83` [c]; os `motivo` de
`apps/seele-app/src/main.rs` [c]):

| nome | quando |
|---|---|
| `sem-espera` | não há espera com aquele token, para aquele MOD e aquela pessoa, dentro do prazo |
| `tipo-recusado` | os bytes não são de um tipo que o MOD declarou aceitar; leva `detectado` quando o produto reconhece o formato (`'heic'`, por exemplo), para o MOD dizer «HEIC não é aceito» |
| `formato-desconhecido` | os bytes não são de formato nenhum que o produto reconheça |
| `arquivo-grande-demais` | passa do teto por arquivo |
| `nao-gravei` | o disco de quem hospeda recusou a escrita |
| `incompleto` | o fluxo terminou antes do que dizia carregar |
| `sessao-encerrada` | a sessão acabou no meio |
| `arquivo-ilegivel` | o arquivo escolhido não abriu |
| `papel-nao-serve` | o arquivo escolhido é de outro papel que o pedido (um som onde o MOD pediu retrato); leva `papel` |
| `arquivos-demais-de-pe` | o MOD já guarda o máximo de arquivos escolhidos ao mesmo tempo |
| `arquivo-nao-esta-de-pe`, `arquivo-de-outro-mod` | o identificador não é de um arquivo guardado, ou é de outro MOD ou de outra sessão |
| `banco-indisponivel` | a tabela dos arquivos escolhidos não respondeu |
| `falhou` | uma falha sem nome; o motivo vai ao `seele.log`, com o `mod_id=` |

**As recusas das contribuições** [c, tarefa 5, Step 7b] — no registro, no
`atualizar` e no `revogar`, com a mesma forma (`e.message` e `e.erro` são o
nome; a frase vai ao `seele.log`):

| nome | quando |
|---|---|
| `ponto-desconhecido` | o ponto não está na tabela de pontos |
| `modo-recusado` | o ponto não aceita o modo pedido — inclusive `decorar`, que não está na 6 |
| `sem-alvo` | o ponto exige alvo (o `pessoa.avatar`) e o pedido não trouxe |
| `conteudo-invalido` | o conteúdo não tem a forma que o ponto exige (o `doServidor` do avatar; a ordem de `pessoas.lista` e de `servidor.navegacao` substituídas) |
| `teto-de-contribuicoes` | o MOD já tem 128 contribuições **sem alvo** de pé (§12) |
| `teto-de-alvos` | o MOD já alcança 256 alvos neste ponto (§12) |
| `contribuicao-ja-existe` | o MOD já tem uma contribuição para este alvo, neste ponto e neste modo: use `atualizar` |
| `contribuicao-de-outro-mod` | o `handle` de `atualizar` ou `revogar` é de outro MOD, ou de outra instância dele |

**Como o nome chega** [c, tarefa 4 passo 5b]: o Rust recusa com um nome
(`FalhaNoMod::Recusado { motivo }`), e um sufixo depois de `:` vira campo —
`tipo-recusado:heic` chega como `erro: 'tipo-recusado'` e `detectado: 'heic'`,
`papel-nao-serve:imagem` como `papel: 'imagem'` (`recusaParaOMod`, `base.js`).
Ele chega pelos dois caminhos: a resposta de um pedido (o `Error` do prelúdio,
com os campos) e o evento `arquivo` de um seletor que recusou (`resultado:
'falhou'`, com `erro` e `detectado`). Um `Error` da casca pode trazer o nome em
`recusa`; as recusas das contribuições ganharam nome na tarefa 5 (a tabela
acima), e as das superfícies ganham na 7 — até lá chegam como `falhou`, com o
motivo no registro. **Até a API 5,
nada muda**: o MOD recebe a frase de tela, como os publicados recebem (o K1-m3
da F5).

**Como a recusa chega ao MOD, com os campos dela** (o I do PERFIS, 09/10). Hoje
o prelúdio rejeita a promessa com `new Error(m.erro)`, e nada além do nome
atravessa (`apps/seele-app/src/executor.rs:1595` [c]). Na 6, o `Error` leva o
nome **e os outros campos da resposta** como propriedades, que é o que a casca
já faz com os `erros` de um formulário: `e.message` e `e.erro` são o nome
(`'tipo-recusado'`), e `e.detectado` é `'heic'`. Vale para toda recusa que
chega à janela pelo mesmo caminho, e não só para o volume: um campo que o
produto acrescentar a uma recusa chega ao MOD sem outra mudança no prelúdio.
O `ok` e o número do pedido não vão. [tarefa 4, passo 5b]

**O HEIC é reconhecido para ser nomeado** [c, tarefa 4 passo 5b]:
`seele_proto::imagem::reconhecido_e_nao_aceito` lê a caixa `ftyp` (`heic`,
`heix`, `mif1`) e o seletor recusa o HEIC das Fotos com `tipo-recusado:heic`, e
não mais com `formato-desconhecido`. O `sniff` não muda: o HEIC continua
recusado, com o nome dele. A prova é um HEIC de verdade, de 8 × 8, feito pelo
`sips` do macOS (`crates/seele-proto/tests/vetores/heic-8x8.heic`). Reconhecer o HEIC só para
nomeá-lo é a tarefa 4 do plano; a conversão no celular, se houver, é da ponte
da F4.

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

**Como nascem** [c, tarefa 8]: a cada `Snapshot` que a janela desenha, o retrato
de agora é comparado com o de antes (`mudancasDaTela`), e só o que mudou vira
evento — nada num temporizador que repita o mesmo estado. O primeiro retrato de
uma sessão não é mudança: o MOD lê o retrato quando sobe. Só os MODs cuja API
tem `eventos-da-tela` os recebem. A fala sai no máximo a cada 250 ms, e o
conjunto mais novo sempre chega (`criarRitmoDaFala`). O `lugar` sai quando
`onde` ou as `regioes` mudam, com a largura e a altura da sessão; o tamanho de
cada região, e o `lugar` quando ele muda, são da tarefa 5.

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

- **Só para quem pode ler aquele canal**, e só para as janelas do mesmo MOD,
  **inclusive a de quem fez o pedido que causou o aviso** (pergunta do PERFIS,
  09/10): uma regra só, e a janela que gravou confere a revisão e ignora o que
  já tem.
- **A carga é um valor JSON** (objeto, lista, texto ou número): a metade de
  servidor passa o valor, o servidor o serializa (o teto de bytes vale sobre o
  texto serializado, e o fio leva texto, `ModAviso.carga: String`), e a janela
  o recebe já lido em `e.carga`.
- **Teto de tamanho e de taxa** [os números esperam a M3]: proposta, 1 KiB
  por aviso e 4 por segundo por MOD, em qualquer janela de um segundo, e até
  16 canais por chamada. Dentro de uma chamada, os avisos já se juntam por
  canal: o mais novo de cada um.
- **`mundo.avisar` responde** `{ ok: true }` ou `{ ok: false, erro }`, com
  `aviso-invalido` (não é `{ canal?, carga }`, o canal não é inteiro positivo,
  ou a carga falta ou o JSON não a escreve), `aviso-grande-demais` e
  `avisos-demais`. A janela recebe o evento `servidor`, com `canal` e a `carga`
  já lida. [c, tarefa 8]
- **O excedente é coalescido por canal, e não descartado** (pedido da MESA,
  09/10). O servidor guarda, por MOD e por canal, só o aviso mais novo que
  ainda não saiu, e o entrega assim que a taxa deixa; os do meio são
  substituídos e contados no `seele.log`. Assim a última revisão sempre chega:
  numa luta, as rolagens de cinco pessoas passam de 4 por segundo, e um aviso
  descartado deixaria uma janela parada na revisão velha.
- **O aviso só sai depois de o quintal gravar** (pedido da MESA, 09/10). Os
  avisos feitos dentro de um `aoPedir` ou de um `aoAcontecer` são segurados
  até o fim da chamada, e só saem se o `dados` gravar; se a chamada lançar, ou
  se o quintal recusar a escrita (cheio), eles são descartados com ela. É a
  mesma regra do quintal, que só grava depois de uma execução bem-sucedida
  (`api/v2.json`, `notes`): um aviso nunca fala de uma revisão que não existe.
- **A mensagem no fio é o `ModAviso`** (ordinal 46, da 1.0, pela F1). O
  servidor o manda a quem **pode ler** o canal, sem exigir que a conexão tenha
  aberto aquele canal: a janela da MESA precisa saber que a mesa do canal 7
  mudou com a conversa de outro canal aberta. [c, tarefa 8]

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
| `mundo.tema` (novo) | §9: só para o MOD cujo manifesto declara `"tema": true` |

**O que as versões 1 a 5 diziam e não é isto** (§13): os 13 verbos de `actions`
da v1 (`mandarMensagem`, `criarSala`, `expulsarPessoa`…) e as 19 leituras de
`reads` (`servidor.pessoas`, `pessoa.apelido`…) **não são expostos à metade de
servidor**: nenhum desses nomes aparece no código fora de `api/` [c]. E o
bloco `own` da v1 chama de `guardar`, `lerArquivo` e `escreverArquivo` o que o
MOD escreve como `dados[chave]`, `arquivos.ler` e `arquivos.escrever`. O
`check-api` passava porque confere o símbolo da direita (`SendMessage`,
`recolher_quintal`), que existe.

### O escopo de um pedido, e a permissão de escrita de cada escopo

**O escopo de servidor já existe no fio, desde 18/09, e não muda nada nele**
(correção da F1 ao §14 desta página): `ModRequest` com `channel =
ChannelId(0)` quer dizer «nenhum canal», porque a chave primária de um canal
começa em 1 (`crates/seele-server/src/mods/pedidos.rs:174-186` [c]); o
contexto chega ao MOD com `channel: null` (`:217-221` [c]); o
`PedidoDeImagem::Ler` usa a mesma convenção (`volume.rs:191` [c]); a janela
repassa o canal que o MOD escolhe (`pedirAoServidor`, `base.js:545` [c]). E o
guia já documenta: «`canal` é um identificador de canal, e `0` quer dizer
"nenhum"» (`docs/como-se-faz-um-mod.md:100-101`, `:119`).

**Por que o PERFIS não mostra nada num servidor só de voz (R5), medido.** Um
cliente sem janela mandou à metade de servidor do PERFIS 3.3.0 publicado um
`view` e um `save` com o canal 0, e os mesmos com o canal 1
(`docs/evidencias/m3-custo-dos-mods/m3-pedido.rs`). **Os quatro voltaram
`ok: true`**, e o `save` sem canal subiu a revisão do perfil para 1. O servidor
e o PERFIS servem o pedido de escopo de servidor. **A causa é a casca comum dos
MODs oficiais**: ela escolhe o canal por `snapshot.open_channel ??
snapshot.channels?.[0]?.id` (PERFIS 3.3.0, `cliente/main.js:84` [c]) — um
campo que nunca existiu (MESA A1) e, sem ele, o primeiro canal de texto — e,
sem canal nenhum, para com «Entre em um servidor com um canal de texto»
(`:281` [c]), sem nunca tentar o 0. O conserto é do porte (o perfil e o tema
são de escopo de servidor; a mesa é de canal), e a API só precisa dizê-lo.

**As regras da v6:**

1. **O escopo é dito no contrato.** `reads` lista `contexto.canal` como
   `ChannelId | null`, e a v6 escreve: `SeeleMods.request(id, 0, valor)` é um
   pedido de escopo de servidor, e o `aoPedir` o recebe com `channel: null`.
   Um dado que é do servidor (o perfil, o tema) é pedido com 0; um dado que é
   de um canal (a campanha da MESA), com o canal.
2. **A permissão de escrita é definida por escopo** (hoje o `write` do
   contexto é `may(person, WriteChannel)` global, qualquer que seja o canal,
   `pedidos.rs:203-205` [c]):
   - **escopo de servidor:** `contexto.write` é a permissão global de escrever
     (`WriteChannel`), e `contexto.admin` é a de administrar o servidor
     (`AdministerServer`). É o que vale hoje, e passa a estar escrito. Uma
     permissão nova («editar o próprio perfil») exigiria migração e tela de
     papéis, e nenhum MOD a pediu;
   - **escopo de canal:** `contexto.write` passa a ser a mesma pergunta que o
     produto faz para publicar naquele canal, `pode_no_canal(pessoa, canal,
     Escrita)` (`crates/seele-server/src/autorizacao.rs:104` [c]): a permissão
     global **e** o papel mínimo do canal;
   - **e o pedido num canal que a pessoa não lê é recusado**, com
     `pode_no_canal(…, Leitura)`, como o histórico. Hoje só se confere que o
     canal existe (`pedidos.rs:191-195` [c]).
   Os dois últimos não mordem hoje — nada grava papel mínimo em canal nenhum
   (`autorizacao.rs:40-44` [c]) —, mas mordem no dia em que um canal privado
   existir, e a v6 congela antes disso.

**Pedido da MESA (item 9):** o orçamento do quintal legível pela metade de
servidor (`dados.uso()`, `dados.teto`), e a recusa por quintal cheio com código
próprio. Entra: é só fachada.

## 9. O tema

O tema da API 4 tem dez nomes escritos em `#tela-sessao` (`base.js:1876-1888`
[c]) e conferidos só como texto sobre fundo (`oTextoAlcancaOFundo`,
`base.js:1928` [c]). O ESTILO mediu o que isso deixa passar (A4). Na 6, **o
tema é dado do servidor**: a metade de servidor o declara, o servidor o guarda
e o manda, e o produto o aplica. O fio já tem a mensagem, `TemaMudou { tema:
Option<TemaDoServidor { mod_id, valores }> }`, ordinal 47, que sai sempre logo
depois do `Session`, inclusive com `None`, e de novo a cada troca
(`crates/seele-proto/src/control.rs:2559` [c]). **O que falta é o runtime**: o
servidor não a produz e o núcleo a ignora (`crates/seele-core/src/state.rs:1647`
[c]). Esta seção responde, item a item, aos bloqueios que o ESTILO listou no
plano do porte (B0 a B9, `docs/plano-api-6.md` do ESTILO); as tarefas são as do
grupo 17 do plano da F3.

### Como o tema chega (B1)

- **A metade de servidor declara com `mundo.tema(valores | null)`.** `valores`
  é o objeto do tema (os nomes do §9, abaixo); `null` tira o tema. A chamada
  confere o tema como a janela confere (os pares, abaixo) e devolve a recusa
  com o par e a razão, em vez de mandar a todos um tema que a casca recusaria.
- **A recusa é valor devolvido, e não exceção** (O1 do ESTILO). `mundo.tema`
  devolve `{ ok: true }` ou `{ ok: false, erro }`, com `erro` da lista fechada:
  `sem-porta-de-tema` (o manifesto não declara `"tema": true`),
  `tema-de-outro-mod` (outro MOD é o escolhido e está ligado), `tema-invalido`
  (não é objeto nem `null`: texto, número, lista), `tema-grande-demais` (o JSON
  passa dos 12 KiB do fio) e, com a tarefa 17e, `contraste`, que leva
  `pares: [{ frente, fundo, razao, piso }]` com **os dois nomes em campos
  separados** (`frente: 'acento', fundo: 'acentoFraco'`), para o MOD pôr cada
  lado no seu campo dos `erros`. Só lança o que o próprio MOD lançar ao ser
  serializado (um `toJSON` que lança, um objeto com ciclo). [c, tarefas 17a a
  17c; `contraste` na 17e]
- **A declaração cai junto com o quintal** (O2). Ela só vale se o `aoPedir` ou
  o `aoAcontecer` terminar: um MOD que declara e lança depois não publica nada,
  e o `TemaMudou` só sai depois de o quintal e o tema gravados, na mesma
  tomada do banco — a ordem que o aviso do servidor também segue (§7.3). [c,
  com teste]
- **O servidor não confere quem pediu** (O3). A porta é o manifesto, e não a
  pessoa: um pedido de quem não administra pode levar o MOD a declarar (a
  redeclaração da E6 do ESTILO, depois da subida do 0.15, quando o tema
  guardado começa vazio). Se o MOD quiser restringir, ele lê `contexto.admin`.
  [c]
- **Funciona nos dois caminhos** em que a metade de servidor roda: o pedido e
  o momento (`aoAcontecer`). [c, com teste]
- **O servidor guarda o tema declarado** no banco dele (migração 15, tabela
  `temas_dos_mods`), e não o MOD. Entre dois reinícios do `seeled`, o
  `TemaMudou` logo depois do `Session` e do `Instancia` leva o tema guardado: a metade de servidor só roda dentro de `aoPedir` e de
  `aoAcontecer`, sem gancho de carga, e um tema que dependesse dela sumiria no
  reinício.
- **A porta do lado do servidor é o manifesto.** Só um MOD cujo `mod.json`
  declara `"tema": true` pode chamar `mundo.tema`. A tela de aceite mostra isso
  numa linha própria («este MOD muda a aparência do servidor»), como o
  alcance. Um MOD sem a declaração recebe a recusa pelo nome. A frase de
  `TemaDoServidor` («quem tem o ponto `servidor.aparencia`») muda para «quem o
  manifesto autoriza».
- **`servidor.aparencia` passa a ser a porta de quem vê.** Ele deixa de ser um
  ponto que a janela do MOD registra (o tema chega antes de qualquer janela) e
  vira o lugar em que a escolha de cada pessoa vale: na gestão de MODs, «o SEELE
  desenha» para a aparência **tira o tema daquele servidor nesta máquina**, e
  a casca deixa de aplicar o `TemaMudou` dele até a escolha mudar. A aba
  DIAGNÓSTICO diz qual MOD pinta a aparência pelo `mod_id` do tema. Ele sai de
  `PONTOS_DE_CONTRIBUICAO` no corte, e a regra de pontos do `check-api` cobra a
  saída nos dois sentidos.
- **Desligar ou desinstalar o MOD manda `TemaMudou { tema: None }`** a todos.
  O tema guardado fica, inativo, porque desligar preserva os dados (ADR 0045):
  religar o MOD o devolve.
- **Um tema por servidor** (decisão do dono, 09/10/2026). O servidor aceita
  `mundo.tema` de um MOD só, o escolhido, e o que fica de fora recebe a
  recusa `tema-de-outro-mod` na própria chamada. Escolhido é o primeiro MOD
  com a porta que declara, e continua sendo enquanto estiver ligado.
  Desligado, a porta fica livre: um servidor com outro MOD de tema ligado não
  fica sem aparência até alguém reinstalar o primeiro. A troca explícita por
  quem administra, na gestão de MODs (habilitar um segundo MOD com a porta
  pede a escolha ali), é da casca, na tarefa 17f, e escreve na mesma coluna.
  [c, tarefas 17a a 17c]
- **Tirar o tema não devolve a porta.** O escolhido que declara `null` deixa o
  servidor sem tema e continua escolhido: um «voltar ao padrão» não pode
  entregar a aparência do servidor a outro MOD sem ninguém decidir. [c, com
  teste]

### As chaves do tema: uma lista fechada (pergunta da F8, 09/10)

**O MOD escreve nomes do contrato, e nunca nomes de CSS.** O tema é um objeto
cujas chaves são os nomes do §9 (`fundo`, `texto`, `acento`, `borda`, `painel`,
`apagado`, os números e as bandeiras, e o que a ampliação somar: `modo`, a
densidade, os tokens por região). O produto traduz cada nome por uma **tabela
fechada** para o token `--seele-*` que ele redefine, como a casca já faz hoje
(`TEMA_DA_API`, `base.js:1844` [c]): uma chave fora da tabela é recusada pelo
nome (`base.js:2038` [c]), e um valor fora da forma (`#rrggbb`, o intervalo do
número, o booleano) também.

**Nenhum nome da tabela leva a `--nucleo-*`.** No celular, o MUDO, o SURDO e o
SAIR leem variáveis `--nucleo-*`, declaradas na raiz a partir dos tokens do
produto; é isso que garante o contraste deles sob qualquer tema, e a garantia
só vale porque o MOD não escreve nomes livres. A F8 põe o guarda de que nenhum
valor da tabela começa com `--nucleo-`.

**O `mundo.tema` confere as chaves pela mesma tabela** [c, tarefa 17j]. Até a
17j ele conferia só a forma (objeto ou `null`) e o tamanho, e um MOD lia `ok`
para um tema que a casca ia recusar — o produto sabendo e não contando. Agora a
tabela é `seele_proto::mods::CHAVES_DO_TEMA`, com a forma de cada valor (cor
`#rrggbb`, inteiro com intervalo, bandeira, escolha), e a recusa volta na
própria chamada:

```js
mundo.tema({ acento: "#331704", "--nucleo-mudo": "#ffffff" })
// → { ok: false, erro: "chave-desconhecida", chave: "--nucleo-mudo" }
mundo.tema({ arredondamento: 25 })
// → { ok: false, erro: "valor-invalido", chave: "arredondamento" }
```

Com mais de uma chave errada, **as duas pontas nomeiam a primeira em ordem
alfabética** — a dos pontos de código, que é a dos bytes UTF-8: o servidor lê o
objeto num `BTreeMap`, e a casca ordena do mesmo jeito antes de conferir (o
`<` do JavaScript compara unidades UTF-16, que discordam acima de U+FFFF). A
primeira chave errada não depende de quem respondeu. **«A mesma
tabela» quer dizer a mesma resposta**: o vetor
`crates/seele-proto/tests/vetores/chaves-do-tema.json` (39 casos) é lido pelo
servidor e pela casca, e os dois têm de aceitar e recusar igual, pela mesma
chave e pelo mesmo motivo; um teste espelho em `frontend.rs` confere que as
duas tabelas têm as mesmas chaves e formas, e outro, que o vetor usa cada chave
e cada escolha da tabela. O vetor achou um defeito da casca: ela conferia com
`in`, que enxerga o protótipo — `fonte: "toString"` era aplicado, e
`constructor` caía nas medidas. Agora é `Object.hasOwn`.

### O que o produto faz com ele, dito ao MOD (B2)

**Um evento `tema` volta à janela do MOD** cada vez que a casca aplica um
`TemaMudou` ou uma prévia: `{ estado: 'aplicado' | 'recusado' | 'nativo',
pares?: [{ frente, fundo, razao, piso }], previa: boolean }`, com os pares na
mesma forma da recusa de `mundo.tema`. `recusado` traz os pares e
os números; `nativo` diz que quem vê escolheu «o SEELE desenha». É o que deixa
o ESTILO dizer na tela o que aconteceu, e não o que ele supõe (o U24 dele).

### Os pares e os tons derivados, em números (B3)

O contrato publica **a lista de pares que o produto confere e o piso de cada
um**, e **a fórmula dos tons derivados**, para a metade de servidor de um MOD
conferir o mesmo que o produto antes de publicar. Os pisos de partida, pela
WCAG e pelos tokens que o produto já cumpre (`apps/seele-app/tests/tokens.rs`):

| par | piso |
|---|---|
| `texto` sobre `fundo`; `texto` sobre `painel` | 4,5:1 |
| `apagado` sobre `fundo`; `apagado` sobre `painel` | 3:1 |
| `acento` sobre `fundo`; `acento` sobre `painel` | 4,5:1 |

**A lista só recusa o que o MOD escolheu.** O `acento` sobre o `acentoFraco`
estava nela, e saiu (tarefa 17e, 09/10): o `acentoFraco` é do produto, que o
deriva, e o derivado **se ajusta** até o acento alcançar 4,5:1 sobre ele.
Recusar um tema por uma cor que o produto mesmo escolheu seria recusá-lo por um
motivo que o MOD não tem como consertar. O par continua garantido — pela
derivação, e não pela recusa.

Os itens do núcleo (o MUDO, o SAIR, o veredito) não entram na lista: eles leem
os tokens do produto, que o tema não alcança (§10).

**Os tons derivados** (tarefa 17e, medido em 09/10). Só existem sob um tema:
**sem nenhuma cor de tema, nada é derivado**, e a sessão sai igual pixel a pixel
à de antes (capturada a 1280 e a 900 de largura, contra o commit anterior).
Sob um tema, em OKLCH:

- **o preenchimento de uma cor** (`acentoFraco` e os `-fraco` das cores de
  estado) tem o matiz da cor, 0,23 do croma dela, e a luminosidade do `fundo`
  um passo de 0,122 na direção da cor; o passo **encolhe** até a cor alcançar
  4,5:1 sobre o preenchimento;
- **o segundo `apagado`** (`--seele-osso-apagado`, que nenhum tema escreve) é
  mantido se já alcança 3:1 sobre o `fundo` e o `painel`, e senão tem a
  luminosidade movida até alcançar, com o matiz;
- **`bordaForte` não é derivado.** A régua do produto (`--seele-linha-forte`,
  `#3a322a`) é escura e continua visível sobre um fundo claro; borda não tem
  piso de contraste aqui, e derivar fica para quando um tema medido mostrar a
  régua sumindo.

**Com as superfícies exatas do produto, os tokens do produto (ΔE 0).** Os
quatro `-fraco` do produto foram escolhidos à mão sobre o `fundo` e o `painel`
dele, e nenhuma fórmula de uma regra só os reproduz: derivados, saíam a
**ΔE2000 de 3,2 a 4,2** (`acentoFraco` `#2f140c` contra `#331704`, a 4,18 —
medido na 17e). Por isso a fórmula tem um atalho: com o `fundo` e o `painel`
iguais aos do produto, o preenchimento de uma cor do produto é o token dela, e
só o que depende de uma cor que o MOD mudou é derivado (um acento azul sobre as
superfícies do padrão ganha o preenchimento dele; as cores de estado continuam
as do produto). **Fora delas, a derivação vale, com ΔE2000 ≤ 4,5** dos tokens
para os preenchimentos e igualdade para as cores de estado que já alcançam —
medido com um `fundo` a um passo do padrão. A promessa de ΔE ≤ 2 em todo lugar
não se cumpre; o contrato é este, na bancada
`apps/seele-app/bancada/tons-do-tema.cjs`, que lê os tokens do `tokens.css`.
O guia publica a mesma função em JavaScript, que a casca usa (`tonsDoTema` em
`ui/base.js`).

### As cores de estado (achado da F8, 09/10)

**As cores de estado do produto não são chaves do tema, e passam a ser
derivadas dele.** São o fósforo (`--seele-fosforo`, quem fala, o nominal), o
azul (`--seele-padrao-azul`) e o vermelho (`--seele-vermelho-alerta`), com os
preenchimentos `-fraco` de cada um. A F8 mediu no protótipo: o «juli fala», em
fósforo, some sobre um `painel` claro de tema. Pôr as três na lista de pares
conferidos recusaria temas claros inteiros, por uma cor que o MOD nem escolhe.

**Decidido em 09/10: sob um tema, o produto deriva cada cor de estado das
superfícies do tema**, garantindo o piso:

- mantém o matiz da cor do produto e move a luminosidade até a cor alcançar
  **4,5:1 sobre `fundo` e sobre `painel`** (o estado aparece como texto: «juli
  fala»), indo para longe das superfícies, até o ponto mais perto da cor do
  produto que alcança; o croma recua só o que for preciso para caber no sRGB.
  O `-fraco` sai da cor derivada pela regra dos preenchimentos acima, com a cor
  sobre o próprio `-fraco` também em 4,5:1;
- com as superfícies exatas do padrão, a derivação **devolve as cores e os
  preenchimentos do produto** (ΔE 0); perto delas, as cores do produto e os
  preenchimentos a ΔE2000 ≤ 4,5, como acima;
- se nenhuma luminosidade servir às duas superfícies, o tema é recusado com o
  par, na mesma forma da recusa de contraste (`{ frente: 'fosforo', fundo:
  'painel', razao, piso }`). Como os pares já pedem `texto` em 4,5:1 sobre as
  duas, isso só acontece num tema que já está no limite — medido: um tema de
  superfícies cinza-médias, que o `texto` já não alcança, é o caso;
- uma cor do produto que não pôde ser lida (uma folha que não carregou) não
  vira recusa nem tom: sem cor medida não há medida;
- **o MOD continua sem escrever nenhuma delas**: o `--seele-vermelho-alerta`
  segue fora de todo caminho de tema, e o produto o deriva só para o que está
  dentro do escopo do tema. Os itens do núcleo leem `--nucleo-*` (§10), e o
  veredito «QUEM ATENDEU NÃO É O DONO DESTE CÓDIGO» não muda de cor com tema
  nenhum.

A função é a mesma dos tons derivados, publicada no guia. **Feito na 17e**,
com teste nos dois sentidos: as cores do padrão devolvem as do produto, e um
tema claro dá ao fósforo, ao azul e ao vermelho derivados 4,5:1 sobre o
`fundo`, o `painel` e o próprio `-fraco` (`tons-do-tema.cjs`); no navegador, o
número da sincronia (fósforo) e o nome de quem usa (azul) saem a 4,52:1 sob o
tema claro, e sem a derivação o fósforo do produto sai a 1,05:1
(`tema-claro-legivel.cjs`). Os apelidos `--seele-sync-*` passaram a ser
redefinidos em `#tela-sessao`: definidos com `var()` na raiz, resolviam lá e não
acompanhavam a cor derivada.

### A prévia (B4)

- `SeeleUI.tema(valores, { previa: true, superficie: id })`: vale só nesta
  máquina, sobre o tema publicado, com o selo «prévia não publicada», e
  pertence à superfície `id`.
- Ela sai quando essa superfície fecha ou é descartada, ou com
  `SeeleUI.tema(null, { previa: true })`, sem fechar a superfície.
- **`SeeleUI.tema(valores)` sem `previa` não existe na 6**: o tema publicado é
  o que a metade de servidor declara. Chamá-lo é recusado pelo nome, com a
  razão.
- No celular, «ver na sessão» recolhe o diálogo e mantém o selo (D-estilo-1).
- A prévia também passa pelos pares, e o evento `tema` volta com `previa: true`.

### O resto do que entra na 6

- **O contraste do núcleo é garantido pelo produto, com qualquer tema** (achado
  do ESTILO, spec-mestre §8). Os itens do núcleo leem os tokens do produto num
  escopo que o tema não sobrescreve. Um guarda prova que os acentos `#331704`,
  `#050403` e `#0a0806` não mudam a cor computada do `#veredito`, do
  `#botao-mudo` apertado e, no celular, do `#voz-mudo` e do `#voz-sair`.
- **`folha` e `dialogo` herdam o tema da sessão**, e um estilo de MOD pode
  citar um token (`cor: { tema: 'acento' }`), resolvido pelo produto (ESTILO
  A16). Cada peça ganha um teste no produto antes de o ESTILO depender dela
  (B8).

### A ampliação: o que entra na 6 e o que espera a API 7 (B5)

Os nomes congelam com o `v6.json`, e o que não entrar espera uma API somada
depois, **que é a API 7** (Servidor 1.1): não há «6.x» na numeração de APIs.
Com a recomendação do ESTILO, que adoto:

| item | onde |
|---|---|
| `modo: 'claro' \| 'escuro'` | **6** |
| a densidade cobrindo a escala `--seele-e*`, sem mudar o nome das duas densidades | **6** |
| tokens por região, com o destino de cada região no celular (§11) | **6**, no contrato; o ESTILO os usa numa 4.x |
| fundo da sessão por mídia | **6 se a M3 com a API 6 o medir no iPhone antes do congelamento** (tarefa 11); senão, **7** |
| tipo dos títulos e dos dados, e escala por escolha | **7**: não medidos |
| família de tipo livre | fora, sempre |

### Os dados do tema ao subir de linha (B9)

Quem hospeda e sobe do Servidor 0.15 para o 1.0 **mantém o quintal do MOD**:
a 0.16 abre e migra a pasta de dados de sempre (a cópia para
`dados/servidor-0.15` é da linha legada), e a tabela `mod_data` é endereçada
pelo identificador do MOD, e não pelo hash do pacote, então o ESTILO 4.0.0
herda o que o `seele/estilo` 3.x gravou. **Isso pede um teste com a F6**
(tarefa 18 do plano): hospedar a 0.16 sobre uma pasta da 0.15 com dados do
`seele/estilo` e ver o `mod_data` intacto depois da migração. **O tema
guardado pelo servidor (B1) é novo na 6**: ele começa vazio, e a primeira
publicação do ESTILO 4.0.0 o preenche a partir do quintal.

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
- **as abas do pé** (SERVIDOR · CONVERSA · PESSOAS · AJUSTES): AJUSTES é o
  único caminho para desligar um MOD. Os botões das abas podem tomar as cores
  do tema, com os pares conferidos, e não aceitam contribuição. O conteúdo de
  AJUSTES recebe o `operador.barra`, abaixo de SEU PERFIL; a gestão de MODs
  continua do produto, fora da região, e o guarda 11 prova que nenhuma
  contribuição a cobre nem a empurra para fora da vista;
- **a pergunta «DE ONDE?»** (`#origem`) e o seletor do sistema;
- **as áreas seguras**: nenhuma superfície de MOD desenha embaixo do entalhe,
  da ilha ou da barra de gestos;
- **44 px e 16 px**: nenhum estilo de MOD desce um controle abaixo de 44 px nem
  um campo abaixo de 16 px de letra;
- **o zoom e os gestos** (`touch-action: manipulation`): nenhum MOD os religa;
- **nenhuma superfície de MOD cobre a faixa de voz nem as abas** (achado
  medido no protótipo da F8, 09/10: o diálogo do ESTILO em tela cheia cobria
  o MUDO e o caminho para AJUSTES, onde se desliga um MOD). «Tela cheia», no
  celular, é a tela acima delas, para `pagina`, `dialogo`, `folha` e `aviso`.
  É a mesma regra do `operador.barra` em AJUSTES: os controles de voz e o
  caminho para desligar um MOD nunca ficam cobertos. **E nunca ficam inertes**:
  a garantia do ADR 0052 de que um modal torna o resto da aplicação inerte
  ganha, no celular, a exceção da faixa de voz e das abas. O foco do teclado
  continua contido no diálogo; o toque no MUDO, no SAIR e em AJUSTES chega ao
  produto.

**O que protege cada item: a cor ou a estrutura** (decisão da orquestradora,
09/10). São duas proteções, e cada item tem a sua:

- **os controles** — MUDO, SURDO, SAIR, AJUSTES, o veredito — ficam sobre
  superfícies que os pares do tema não conferem, e por isso **leem as cores do
  produto** (`--nucleo-*`, a placa), que o tema não alcança;
- **o apelido verificado nas linhas** (`pessoas.lista`, `pessoa.cartao`,
  `conversa.mensagem`) lê o `texto` e o `painel` do tema, que os pares garantem
  a 4,5:1. O que o protege é **a estrutura**: o lugar reservado à esquerda da
  linha, o miolo do MOD numa caixa fechada que é irmã dele e nunca mãe, e o
  nome lido da chave pelo produto (§3). Um apelido falso desenhado por um MOD
  fica ao lado do verdadeiro, e não no lugar dele.

**A saída de um tema ruim é do produto.** No celular, a aba AJUSTES e a página
MODS DESTE SERVIDOR leem `--nucleo-*`, e não o tema, como CONFIGURAÇÕES no
desktop: quem escolheu um tema ruim sempre consegue sair dele (desenho da F8).

**O guarda 11** prova, para cada item, que ele não está dentro de nenhuma
região, que nenhuma contribuição o cobre e que o tema não o alcança. **No
celular, prova também que nenhuma superfície aberta cobre a faixa de voz nem
as abas**: com uma `pagina` imersiva, um `dialogo`, uma `folha` expandida e um
`aviso` abertos, o retângulo de cada um termina acima da faixa, e um toque no
MUDO e em AJUSTES chega ao produto. **No desktop, o guarda mede antes de
concluir**: se um `dialogo` aberto deixa o MUDO e o SAIR da barra do operador
à vista e clicáveis. Se não deixar, é o mesmo conserto, e a mesma regra. Agora
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
| `servidor.navegacao` | aba SERVIDOR, que ocupa a célula inteira e leva as salas, os canais, as entradas «DOS MODS» e a troca de servidor | largura da tela; controles de 44 × 44 | aplicada; as entradas «DOS MODS» vão na aba |
| `operador.barra` | **existe, no alto da aba AJUSTES**, logo abaixo do cabeçalho SEU PERFIL (proposta da F8, aceita em 09/10). O MUDO, o SURDO e o SAIR continuam na faixa de voz, que é do núcleo | largura da tela; controles de 44 × 44, como as outras regiões do celular; **altura limitada, com rolagem por dentro**, para a gestão de MODs de AJUSTES continuar à vista (o número sai da M3 no iPhone) | aplicada; a `pessoa.capa` do operador vai no cabeçalho SEU PERFIL (D-capa-1) |
| `canal.cabecalho` | existe, compacto (`:558-561`) | botões de 44 px | aplicada |
| `conversa.mensagem` | existe | — | aplicada |
| `compositor` | existe; campos de 16 px | botões de 44 × 44 | aplicada |
| `pessoas.lista` | gaveta PESSOAS, célula inteira (`:397-407`) | cartão de 44 px | aplicada; a capa da linha fica parada |
| `status.barra` | **não existe** (decidido com a F8, 09/10): a casca do celular tira o rodapé da telemetria, e as medidas dele foram para a fileira da CHAMADA — que tem a altura exata das abas — e para AJUSTES › CONEXÃO | — | **`regiao-ausente`**: a casca declara as sete outras em `CELULAR.regioes`, e o MOD que contribui aqui recebe o motivo, e a aba DIAGNÓSTICO conta |
| `chamada.palco` | **vista alternativa** à grade, que vira uma coluna de cartões de 160 px (`:531-541`) | largura da tela; o MOD recebe o tamanho (§7.2) | aplicada |
| `pagina` | célula inteira; a imersiva **não cobre a faixa de voz nem as abas** | — | [inf] hoje cobre: é `fixed; inset: …; z-index: 30` sem área segura de baixo (`mods-superficies.css:142-146`). Conserto na tarefa 7 |
| `folha` | folha de baixo, acima da faixa e das abas | até a altura do polegar, expansível até o alto, sem cobrir a faixa nem as abas | — |
| `dialogo` | **tela cheia acima da faixa de voz e das abas**, ações fixas embaixo | a área entre o alto e a faixa | **[medido pela F8, 09/10]** no protótipo, o diálogo do ESTILO em tela cheia cobria o MUDO e o caminho para AJUSTES. Conserto na tarefa 7, guarda na 9 |
| `aviso` | canto de baixo, acima da faixa e das abas, com a área segura (`celular.css:79-84`) | `min(420px, 100vw-32px)` | — |

**O desenho do celular é da F8** (D-celular-1, spec-mestre `fd4d298f`): ela
redesenha a casca de uma mão com paridade com o desktop, e as âncoras
`celular.css:…` acima são do desenho de `arquivo/mobile-ios`, que ela
substitui. O contrato fixa o nome de cada região e o que ela faz com uma
contribuição; o lugar de cada uma na casca nova é da F8, e esta tabela
acompanha. A primeira aba se chama SERVIDOR, e não SALAS: é rótulo de tela, e
não muda o contrato.

**Uma contribuição para uma região que a casca não tem é recusada com motivo
nomeado** (`contribuicao` com `motivo: 'regiao-ausente'`), e não some calada.
A aba DIAGNÓSTICO conta. **Na 0.16, o celular tem sete das oito regiões: o
`status.barra` não existe nele** (medido e decidido com a F8, 09/10: a casca
do celular tira o rodapé da telemetria, e uma barra de estado escondida a dois
toques em AJUSTES › CONEXÃO prometeria o que não cumpre). A casca do celular
declara as sete em `CELULAR.regioes`, e um MOD que contribui no `status.barra`
recebe `regiao-ausente` — sabe antes de escrever, pela tabela acima e pelas
`regioes` do retrato. No PC no navegador (a casca larga), as oito valem, como
no desktop. A regra vale também para a casca futura que não tiver uma região:
uma região da API 7 numa casca 1.0, ou o executor do navegador da F4 se ele
não desenhar alguma. [c, tarefa 5, Step 8; a declaração das sete é da F8] O MOD sabe onde está pelo `onde` e pelas `regioes` do
retrato, e pelo evento `lugar` quando o aparelho deita.

**Pontos ainda não medidos, que a F4 e a M3 no iPhone respondem:**

- o custo de capas animadas e de fundo por mídia num iPhone (o teto de memória
  de uma aba é muito menor, e passar dele encerra a aba);
- **o som do palco quando a vista alterna** (pergunta da MESA, 09/10): no
  celular, voltar do palco para a grade de vídeo **não tira o palco da
  tela** no sentido da regra «fora da tela, a declaração não liga o som». A
  regra vale para o lugar desmontado (a superfície fechada ou descartada, o
  cartão que a casca não desenha); o palco escondido pela vista alternada
  continua montado, e o som dele continua, com `repetir` ou sem ele,
  enquanto a pessoa estiver na chamada. Sair da sala desmonta o palco, e o
  som para;
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
| mensagem do MOD à janela | 12 KiB | `executor.rs:70`; `control.rs:2711`, `:2830`; `base.js:566` | **fica 12 KiB por mensagem**, e a superfície ganha a montagem por partes (abaixo) |
| contribuições por MOD | 128 | `mods-contribuicoes.js:91` | **muda de modelo** (abaixo): os pontos por pessoa deixam de gastar o teto agregado |
| nós de uma contribuição | 64, para **todo** ponto: `montarContribuicao` montava com o orçamento do cartão, e o `perfil` da tabela de pontos não era lido [c, até a tarefa 5] | `base.js`, `PERFIS_DE_RENDER` em `mods-regiao.js` | **o `perfil` de cada ponto vale** (decidido pela orquestradora, 09/10): `pessoa.detalhes` monta com o de superfície (**4096 nós**: um painel de uma pessoa por vez, aberto sob demanda); **as regiões ficam no do cartão (64) até a M3 medir**, no desktop e no iPhone. Um guarda reprova um `perfil` que `montarContribuicao` não lê |
| superfícies | 12 | `mods-superficies.js:204` | M3 |
| avisos | 4 | `mods-superficies.js:206` | fica |
| nós por superfície / fundura / campos / telas / mídias | 4096 / 16 / 128 / 4 / 24 | `mods-regiao.js:193` | M3 |
| bytes de mídia por superfície | 24 MiB declarados, **10 MiB na forma `midia`** | `mods-regiao.js:194`, `:2159-2161`, `:2226` | §13: a 6 diz o que é entregue |
| região | 512 nós, 8 de fundura, 10 MiB | `mods-regiao.js:33-65` | sai com `regiao` (§2) |
| cartões | 64 pessoas, 64 nós, 6 de fundura, 64 mídias, 20 MiB somados | `mods-regiao.js:104-131` | troca por `pessoa.cartao` com `alvo` (R2) |
| som decodificado | 64 MiB por bolso | `mods-regiao.js:88`, `:148` | M3 e K5 |
| arquivos escolhidos | 4 de pé, 10 MiB cada | `main.rs:2438`, `:2444` | fica |
| imagem por volume | 10 MiB | `midia_de_mod.rs:59` | fica; a redução no Rust (R3) entra |
| pedidos em voo / fila da janela | 8 / 128 | `executor.rs:1505`; `base.js:535` | M3 |
| **novos**: capas animadas, quadros por capa, aviso do servidor | — | — | M3 (§4, §7.3) |

**O teto de contribuições muda de modelo antes do congelamento** (o bloqueio C
do PERFIS, 09/10). Com cartão, avatar e capa por pessoa, 128 por MOD dão **42
pessoas**, e uma sala vai até 250 (`control.rs:77` [c]). O modelo da 6:

- **os pontos por alvo** (`pessoa.cartao`, `pessoa.avatar`, `pessoa.capa`,
  `pessoa.identidade`, `pessoa.detalhes`, `pessoa.acoes`, `canal.item`,
  `sala.acoes`) **não contam no teto agregado**. Cada um aceita **uma
  contribuição por alvo, por MOD e por modo** (registrar de novo para o mesmo
  alvo é `atualizar`, e não uma segunda; o mesmo MOD pode substituir o cartão
  de uma pessoa e acrescentar à linha dela, que são duas coisas), e até um teto
  de alvos por ponto, maior que a sala (partida: 256) [c, tarefa 5, Step 7];
- **o teto agregado** (128 por MOD) fica para os pontos sem alvo: as regiões,
  as entradas de navegação, as ferramentas do compositor;
- **o modelo está decidido** (dono, 09/10/2026); os números (256 alvos, 128
  sem alvo) **esperam a M3 com a API 6**, que mede a lista de pessoas com 250
  cartões de MOD (tarefa 11). Até lá, o laboratório de cada MOD os tem como
  opções, e não como constantes (o J do PERFIS): os mesmos números valem para
  o aviso do servidor (1 KiB, 4 por segundo) e para as capas animadas.

**A montagem por partes** (MESA item 1, PERFIS R8; decidido aqui em 09/10, a
pedido da MESA, porque muda o desenho da ficha): uma superfície montada ganha
`superficie.atualizar(chave, subarvore)`, que troca o nó de `chave` e tudo o
que está sob ele, numa mensagem própria. **O teto por mensagem continua 12
KiB; o teto da superfície passa a ser o de nós (4096) e o de bytes de mídia**,
e não o da mensagem. Uma ficha cheia se monta em partes: o esqueleto numa
montagem, cada seção (inventário, perícias, notas) num `atualizar`. Uma chave
que não existe é recusada pelo nome; uma subárvore que leva a superfície além
dos 4096 nós é recusada com os números. A montagem inteira (`montar`)
continua, e substitui tudo. É também o que tira a remontagem da prévia do
editor do PERFIS a cada tecla (o bloqueio F dele): a prévia é um nó com chave,
atualizado sozinho.

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
| **o exemplo `servidor-com-mod.rs` instala o pacote por identificador, e o servidor o procura por hash**: a metade de servidor recusa todo pedido com «No such file or directory» | `crates/seele-server/examples/servidor-com-mod.rs`; `RaizesDosMods::pacote_de`, `lib.rs:137-138` | **[medido]** na M3 (ADR 0056). **Defeito**: conserto do exemplo e do guia na fase de código (tarefa 15 do plano), com o teste que instala pelo caminho do exemplo |

### As regras da v6 que vêm destes achados

Os achados da tabela acima viram regras, e cada regra tem guarda no
`check-api` (tarefa 2 do plano), cobrada a partir da v6:

1. **Todo nome da metade de servidor que a v6 lista é um nome que o runtime
   expõe.** Os blocos `own` e `world` (e qualquer outro que a metade de
   servidor leia) são conferidos contra as globais e os membros que
   `crates/seele-server/src/mods/mod.rs` registra (`ctx.globals().set("…")` e
   `<objeto>_js.set("…")`), e não contra um símbolo qualquer do repositório. É
   a regra que teria pego os 13 verbos e as 19 leituras da v1: o símbolo da
   direita existia, e o nome da esquerda nunca chegou ao MOD.
2. **Todo teto que a v6 declara é o teto que o produto aplica.** Os números de
   `superficies.limites`, `contribuicoes.teto` e os bolsos de mídia são
   conferidos contra as constantes do renderer (`LIMITES_DA_REGIAO`,
   `PERFIS_DE_RENDER`, `LIMITES_DO_CARTAO` em `mods-regiao.js`;
   `TETO_DE_CONTRIBUICOES` em `mods-contribuicoes.js`). E, porque um número
   certo numa constante que ninguém consulta é o caso da forma `midia`, uma
   bancada prova o comportamento: a mídia que cabe no teto declarado da
   superfície monta, e a que passa dele é recusada com os números (tarefa 7).

## 14. O que a API 6 pede ao contrato de fio

O spec-mestre lista dois itens da F3 no contrato (§3): o 14
(`APIS_ACEITAS = [6]`) e o 15 (`ModAnunciado.api` e as APIs no `Hello`). O
rascunho acrescentou três candidatos. **Encaminhados em 08/10:** o aviso da
metade de servidor (`ModAviso`) e o tema como dado do servidor (`TemaMudou`,
um JSON opaco com teto de 12 KiB no fio) vão para o contrato da 1.0, pela F1;
o segundo, os pedidos de escopo de servidor, **não muda o fio**, porque o
escopo já existe (§8). A tabela abaixo fica como o registro do porquê. Pela D-fio-6, mensagem nova pode chegar numa 1.1
sem quebrar nada; o que está em jogo é se ela é **Servidor 1.0** ou **1.1 com
API 7**.

| candidato | quem pede | o que muda no fio | se ficar de fora da 1.0 |
|---|---|---|---|
| **o aviso da metade de servidor às janelas** (§7.3) | MESA 2, PERFIS R4, ESTILO §3 | uma mensagem servidor → cliente, `ModAviso { mod, canal?, carga }`, com teto | os três continuam em laço até a API 7; no celular, a MESA custa ~20 MB/h |
| ~~**pedidos endereçados ao servidor**~~ (§8) | PERFIS R5 | **nada**: o escopo de servidor já existe (`ChannelId(0)`), correção da F1 | — (a causa do R5 é a casca dos MODs oficiais, medido, §8) |
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
5. **As globais do contexto são as mesmas nos dois alvos.** A F4 extraiu a
   volta do executor para `crates/seele-motor` (merge `d6d1a115`), e a tabela
   `crates/seele-motor/tests/casos.json` roda nas duas pontas, com uma sonda
   das globais. A única diferença medida é o **`Atomics`**: o QuickJS do
   `wasm32-wasip1` não o expõe, e sem threads ele não serve para nada em
   nenhum dos dois. **Decidido em 09/10: a API 6 tira o `Atomics` também do
   desktop**, e a lista `globais_so_do_nativo` da sonda fica vazia. Um MOD
   que dependesse dele rodaria no desktop e quebraria no celular, que é o
   contrário de «um contrato só». Nenhum dos três MODs oficiais o usa [c,
   lido nos três repositórios]. O `SharedArrayBuffer` fica: existe nos dois, e
   sem threads é só um buffer. [tarefa 3, passo 5b]

**A superfície que ele expõe é a deste documento, e é a mesma do desktop**:

- o prelúdio inteiro, hoje em `crates/seele-motor/src/preludio.rs`
  (`SeeleMods.request`, `SeeleUI.*`, os quatro temporizadores, o `console` da
  0.15.1), com `SeeleUI.retrato()` no lugar de `SeeleMods.snapshot()`;
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

**As versões dos portes** (D-mods-1, spec-mestre `0703582a`): os três MODs
portados para a API 6 saem como **4.0.0**, e não 3.4.0. A linha 3.x continua
sendo a do Servidor 0.15 (pode ganhar uma 3.3.2, uma 3.1.2), e as duas convivem
no catálogo.

### PERFIS (R1 a R13)

| # | pedido | situação | por quê |
|---|---|---|---|
| R1 | `pessoa.capa` com legibilidade, halo, estados, movimento reduzido e custo | **entrou** (§4) | é a decisão do dono; os números de redução, quadros e capas animadas ficam para a M3 |
| R2 | cartão por pessoa, sem gastar o teto, citando a imagem da pessoa | **entrou**: `cartoes` sai e o cartão é `pessoa.cartao` com `alvo`, por pessoa; a citação (`daPessoa`); e os pontos por alvo deixam de gastar o teto agregado (§12; os números com a M3) | decisão do dono de 08/10, e o modelo do teto em 09/10 |
| R3 | bolsos por bytes decodificados, redução no Rust | **entrou** (§12) | o custo é o quadro decodificado, e não o arquivo |
| R4 | eventos da tela, retrato projetado, aviso do servidor | **entrou** (§6, §7); o aviso vai no fio da 1.0 (§14) | — |
| R5 | pedidos endereçados ao servidor | **entrou como regra da v6, sem mudar o fio** (§8): o escopo de servidor (`ChannelId(0)`) já existe e passa a estar no contrato, e a permissão de escrita de cada escopo fica definida | medido: o servidor e o PERFIS servem o pedido sem canal; quem não manda o 0 é a casca comum dos MODs oficiais, e o conserto é do porte |
| R6 | formulários no diálogo | **entrou** (§5) | — |
| R7 | `contribuicoes.atualizar(handle, conteudo)` | **entrou** [c, tarefa 5, Step 6]: o conteúdo muda no mesmo nó (o renderer reconcilia por chave), a linha com cartão nunca volta à nativa no meio, e o avatar troca de foto só quando a nova está decodificada — se ela falhar, a velha fica e o MOD recebe o `midia` de sempre. Só o dono atualiza (as três perguntas do `revogar`), e o conteúdo novo passa pela conferência de forma do registro | evita o redesenho nativo entre revogar e registrar |
| — | o orçamento de `pessoa.detalhes` (achado da tarefa 5) | **muda na 6**: o `pessoa.detalhes` do PERFIS passa a montar com **4096 nós** (o perfil de superfície, que a tabela prometia e não valia), e uma declaração que passa do teto chega como `contribuicao` com `motivo: 'orcamento'`, o perfil, o teto e o tamanho (§7.1). As regiões ficam em 64 até a M3. Na 0.15.x nada muda | §12 |
| R8 | montagem parcial nas superfícies | **em aberto** com o teto de 12 KiB (§12) | a MESA pede o mesmo (item 1); a forma (atualizar por chave ou montar em partes) é a primeira decisão de desenho da tarefa 7 |
| R9 | a falha dita a quem pode agir | **entrou**: `midia` com ponto e alvo (§7.1); a recusa `mensagem-grande` vai ao `seele.log` | — |
| R10 | o apelido verificado à vista | **entrou** no núcleo (§10); e **muda a linha do PERFIS**: com `pessoa.cartao` substituído, o apelido verificado sai do «⋯» e fica à esquerda do cartão, à vista, com o botão de moderação (§3, tarefa 5). Quem porta a 4.0.0 desenha o cartão sabendo que o apelido verdadeiro está ao lado, e não precisa repeti-lo | D-nucleo-1 |
| R11 | presets de animação para moldura de retrato e capa; movimento reduzido em toda região | **entrou** a regra de movimento em toda região; **não entrou** o preset novo | a moldura é desenho do MOD, com o vocabulário de estilos que já existe; um preset novo é decisão de estética (`specs/07`), e não de API |
| R12 | `folha` para o perfil | **entrou** (§5) | — |
| R13 | capacidades verdadeiras por plataforma | **entrou** (§2) | — |

### MESA (itens 1 a 18)

| # | pedido | situação | por quê |
|---|---|---|---|
| 1 | montar sem esbarrar em 12 KiB | **em aberto** (§12), com o R8 do PERFIS | — |
| 2 | o servidor avisar as janelas | **entrou**, no fio da 1.0 (§7.3, §14) | — |
| 3 | eventos da tela com o retrato escrito no contrato | **entrou** (§6, §7.2) | — |
| 4 | `repetir`, som que não morre com a página, início pela posição do servidor | **entrou** `repetir: true` (decisão do dono de 08/10); **em aberto** o som que segue com a página fechada e o início pela posição | a vida do som fora da página é a mesma pergunta do som com a tela bloqueada (item 14), e espera a medida |
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
| 3 | tema como dado do servidor; aviso do servidor; `servidor.aparencia` como porta; prévia do produto | **entrou** tudo; o tema como dado e o aviso vão no fio da 1.0 (§14) | — |
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
| P49-10: a frase dos cartões da API 3 | sai com a capacidade `cartoes` (decisão de 08/10) |
| K2: `midia`/`falhou` quando o retrato não decodifica | **entrou** (§7.1) |
| K4 e K5: o som grande demais e o som de contribuição recusado no tocar | **entrou** o texto público, quando os lotes chegarem |
| o contrato de `tocando` (P49-6, `563bc37`) | **entrou** no texto da 6, como está no `api/README.md`, com `repetir: true` (decisão de 08/10) |
| `capture`/`playback` no `Snapshot` | **entrou**: ficam fora da lista branca (§6) |

### Os planos do porte (09/10): o que estava aberto e travava

**MESA** (`docs/plano-api-6.md` dela, §4):

| # | pergunta | resposta | onde |
|---|---|---|---|
| 1 | o `ModAviso` no fio da 1.0 | está, ordinal 46, pela F1 | §7.3, §14 |
| 2 | o aviso acima do teto | **coalescido por canal**: o mais novo de cada canal sai assim que a taxa deixa | §7.3 |
| 3 | o aviso antes de o quintal gravar | **só sai depois de o quintal gravar**; se a chamada lança ou o quintal recusa, ele cai junto | §7.3 |
| 4 | a leitura por `volume` no `fundo` da `tela` | **sim**: toda mídia do servidor aceita `transporte: 'volume'` | §5 |
| 5 | `contexto.sala` | **preenchido pelo servidor em todo pedido**, pela presença; a janela não o manda | §5 |
| 6 | o som do palco quando a vista alterna no celular | **continua**: o palco escondido não é lugar desmontado | §11 |
| 7 | a montagem parcial | **`superficie.atualizar(chave, subarvore)`**; 12 KiB por mensagem, 4096 nós por superfície | §12 |
| — | `pessoa.detalhes` montado por quem vê | retirado pela MESA: cada janela já registra só o que aquela pessoa vê | — |

**ESTILO** (`docs/plano-api-6.md` dele, §8): o B0 é a lacuna do plano (o lado do
tema não tinha tarefa), e virou o grupo 17 do plano da F3. Os outros:

| # | pergunta | resposta | onde |
|---|---|---|---|
| B1 | como a metade de servidor declara o tema; quem o guarda; desligar; quem tem a porta; dois MODs | `mundo.tema(valores \| null)`; o servidor o guarda no banco; desligar manda `None`; a porta é o manifesto (`"tema": true`); um tema por servidor, o escolhido (decidido em 09/10); **feito no servidor** (17a a 17c) | §9 |
| B2 | o resultado da aplicação | o evento `tema`, com `aplicado`, `recusado` (com os pares) ou `nativo` | §9 |
| B3 | os pares e a fórmula dos derivados | a lista só com o que o MOD escolhe; a fórmula, em OKLCH: com as superfícies exatas do padrão, os tokens do produto (ΔE 0); fora delas, as cores de estado do produto e os preenchimentos a ΔE2000 ≤ 4,5 — o ΔE ≤ 2 em todo lugar não se cumpre, medido (17e) | §9 |
| B4 | a prévia | `{ previa: true, superficie: id }`; sai com a superfície ou com `tema(null, { previa: true })`; sem `previa`, não existe | §9 |
| B5 | a ampliação | na 6: `modo`, a densidade completa e os tokens por região; o fundo por mídia se medido a tempo; tipo e escala na API 7 | §9 |
| B6 | `listarNaBarra` | continua; no celular, vale na aba SERVIDOR | §3 |
| B7 | o terceiro botão e o erro de dois campos | `confirmar` em qualquer `botao`; `formulario.erros` com as duas chaves | §5 |
| B8 | as peças só no texto | cada uma com teste no produto, na tarefa 17 | §9; plano, 17 |
| B9 | o quintal ao subir do Servidor 0.15 para o 1.0 | mantido pela leitura (o `mod_data` é por identificador); a prova é a tarefa 18, com a F6 | §9 |

**PERFIS** (`docs/plano-api-6.md` dele):

| # | bloqueio | resposta | onde |
|---|---|---|---|
| A | o `daPessoa` sem tarefa | tarefa 6, passo 4b | §4; plano, 6 |
| B | o `contribuicoes.atualizar` sem tarefa | tarefa 5, passo 6 | plano, 5 |
| C | 128 contribuições dão 42 pessoas | **o teto muda de modelo**: os pontos por alvo não contam no agregado (uma por alvo e por MOD, até 256 alvos por ponto, a medir) | §12; plano, 5 passo 7 |
| D | o sujo com o rascunho reaberto | o campo declara `valor` (o rascunho) e `inicial` (o gravado) | §5; plano, 7 |
| E | o tipo da carga do `ModAviso`, e se a janela de quem gravou recebe | um valor JSON, serializado pelo servidor; e sim, ela recebe | §7.3; plano, 8 passo 4a |
| F | a montagem parcial e a prévia a cada tecla | `superficie.atualizar(chave, subarvore)`: a prévia é um nó com chave | §12; plano, 7 passo 5c |
| G | os nomes das recusas (o HEIC, o tipo recusado) | a lista fechada do §7.1, com `tipo-recusado` levando `detectado: 'heic'` | §7.1; plano, 4 passo 5b |
| H | o `regiao-ausente` sem tarefa | tarefa 5, passo 8 | §11; plano, 5 |
| — | os ausentes no diretório | corrigido no §6: o diretório lista só quem está presente | §6 |

### A segunda volta dos planos do porte (09/10)

**ESTILO** (plano `9c35359` do `revisao-0.16` dele):

| # | pergunta | resposta | onde |
|---|---|---|---|
| O1 | a recusa de `mundo.tema`: valor ou exceção; o `par` como texto ou os dois nomes | **valor devolvido**: `{ ok: false, erro }`, nunca exceção por recusa; o par vem com **os dois nomes em campos separados**, `{ frente, fundo, razao, piso }`, na recusa e no evento `tema` | §9 |
| O2 | o `mundo.tema` cai com o quintal quando o `aoPedir` lança | **cai**: a declaração só vale se a chamada terminar, e o `TemaMudou` sai depois do quintal e do tema gravados | §9 |
| O3 | o `mundo.tema` no pedido de quem não administra | **passa**: a porta é o manifesto, e o servidor não confere quem pediu; o MOD restringe, se quiser, pelo `contexto.admin` | §9 |
| O4 | «um tema por servidor» ainda como proposta na 17c | **decidido** (D-api-5); a marca saiu do rascunho e do plano, e a regra está no servidor | §9; plano, 17c |
| O5 | os números da fórmula e do fundo por mídia | **com a M3**, e o ESTILO não depende deles para o porte | §9, §12 |

**PERFIS** (plano `de4d88a`):

| # | pergunta | resposta | onde |
|---|---|---|---|
| I | como o `detectado` chega ao MOD | o prelúdio rejeita com um `Error` que leva o nome **e os outros campos da resposta** (`e.erro`, `e.detectado`), como a casca faz com os `erros` | §7.1; plano, 4 passo 5b |
| J | os números (256, 128, 1 KiB, 4/s, as capas) | **com a M3**; o laboratório os tem como opções | §12 |

### As perguntas da F8, a tela do celular (09/10)

| pergunta | resposta | onde |
|---|---|---|
| o `operador.barra` no celular, que o rascunho recusava com `regiao-ausente` | **aceita a proposta da F8**: ele mora no alto da aba AJUSTES, abaixo de SEU PERFIL, com os tamanhos das outras regiões do celular e altura limitada, para a gestão de MODs continuar à vista; a capa do operador vai no cabeçalho SEU PERFIL | §11; plano, 5 passos 8 e 8b |
| a regra das chaves do tema, para o `--nucleo-*` ficar fora do alcance | **lista fechada de nomes do contrato**, traduzidos por uma tabela fechada para `--seele-*`, como a casca já faz; o `mundo.tema` confere pela mesma tabela (`CHAVES_DO_TEMA`), com `chave-desconhecida` e `valor-invalido`; **feito na 17j** | §9; plano, 17j |
| o rótulo da primeira aba | SERVIDOR, e não SALAS; rótulo de tela, sem mudar o contrato | §11 |
| uma superfície de MOD cobria a faixa de voz e as abas (medido no protótipo) | **nenhuma superfície as cobre**: «tela cheia», no celular, é a tela acima delas; regra do núcleo e do guarda 11; no desktop, o guarda mede o MUDO e o SAIR com um `dialogo` aberto | §5, §10, §11; plano, 7 e 9 |
| as cores de estado fora dos pares (o «juli fala» some num `painel` claro) | **derivadas das superfícies do tema**, com o matiz do produto e 4,5:1 sobre `fundo` e `painel`; o padrão reproduz os tokens; o MOD não as escreve; **feito** | §9; plano, 17e |

## 18. Em aberto

**Para a M3:** os limites do §12, os novos (capas animadas, quadros, aviso do
servidor) e os do celular.

**Para o dono e as sessões de MOD:** decididos em 08/10 (tabela no alto):

1. ~~`pagina` e `painel` contra `folha`~~: a `folha` substitui o `painel`.
2. ~~`regiao`, `cartoes` e `tema`~~: `regiao` e `cartoes` saem; `tema` fica,
   ampliado, com `servidor.aparencia` como porta.
3. ~~O m-4 da pendência 49~~: `repetir: true` por contrato
   (`AudioBufferSourceNode.loop`), e a exceção do som que terminou fica como
   está até lá. O que muda com ele: um som com `repetir: true` volta ao começo
   no fim sem redesenho, e o MOD recebe `terminou` só de um som sem `repetir`.
4. ~~Os três candidatos ao fio (§14)~~: o aviso e o tema vão ao contrato da
   1.0, pela F1; o escopo de servidor já existia (§8).
5. ~~Os membros fora do ar no retrato (§6)~~: ficam de fora.

E em 09/10: ~~dois MODs com a porta do tema~~ (um tema por servidor) e ~~o
modelo do teto de contribuições~~ (os pontos por alvo fora do agregado; os
números com a M3).

**Ainda em aberto, e de quem:** os itens da ampliação do tema (§9), com o
ESTILO no porte; a montagem parcial e o teto de 12 KiB (§12), com a M3; o som
fora da página e com a tela bloqueada (§11), com a medida da F4.

**Para a orquestradora:** o WebView2 (Windows) e o iPhone na M3; a forma de
rodar o MOD no navegador (F4), contra o §16.
