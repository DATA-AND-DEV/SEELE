# A superfície que um MOD enxerga

Um arquivo por versão, e **nenhum deles é editado depois de publicado**.

`api/congeladas.sha256` guarda o SHA-256 de cada um, e o teste
`a_api_publicada_nao_se_edita` (em `crates/seele-conformance`) reprova o
`cargo test` — e com ele o portão da publicação — quando os bytes de uma versão
listada mudam ou quando aparece uma `vN.json` fora da lista. Mudar o que uma
versão promete é publicar a seguinte, e nunca editar esta — a v3 e a v4,
editadas antes de o guarda existir, ficaram congeladas como estão, com a
história no comentário da lista.

## O que muda para quem escreve MOD nesta versão

**Nada disto está num `vN.json`.** A superfície congelada é a mesma, e um MOD
das APIs 3, 4 e 5 carrega como carregava (`APIS_ACEITAS = [5, 4, 3]`, em
`crates/seele-proto/src/mods.rs`, na v0.15.0 e aqui). O que muda, na versão
depois da 0.15.0, é o executor da metade de janela e o som, e quem escreve MOD
precisa saber disso para o MOD rodar igual nas duas.

> **Emendado em 15/10/2026 — o corte da API 6 (0.16).** Isto vale para a linha
> 0.15. No app 0.16, `APIS_ACEITAS = [6]`: um MOD das APIs 3, 4 e 5 é recusado
> pelo nome, e roda no Servidor 0.15, pelo launcher.

- **Na metade de janela, `console` existe, em qualquer API.** `console.log`,
  `info`, `warn`, `error` e `debug` — e `dir`, `table`, `trace` e o `assert`
  que falha — viram uma linha no `seele.log` da máquina que roda o MOD, com o
  id dele num campo próprio (`mod_id=`); `group`, `time`, `count` e `clear`
  não fazem nada. O `debug` não é gravado pelo filtro padrão do `seele.log`.
  Como ler essas linhas está em
  [`docs/como-se-faz-um-mod.md`](../docs/como-se-faz-um-mod.md), «Quando o MOD
  não faz o que devia».
- **A metade de servidor não tem `console`**, nesta versão nem na 0.15.0. Lá,
  `console.log(…)` lança `ReferenceError`, e um `aoAcontecer` que lança
  desliga o MOD. O registro dela é `mundo.registrar(texto)`: uma linha INFO
  no registro de quem hospeda, com o mesmo `mod_id=`, cortada em 500
  caracteres ([`docs/como-se-faz-um-mod.md`](../docs/como-se-faz-um-mod.md),
  «`mundo` — rede, relógio, registro»).
- **Na 0.15.0, a metade de janela também não tem `console`.** Lá,
  `console.warn(…)` é um `ReferenceError`, e um MOD que o chame fora de um
  `try` quebra, na mesma sala, para quem não atualizou. Quem precisa rodar nas
  duas versões escreve:

  ```js
  if (typeof console !== "undefined") console.warn("o retrato não veio");
  ```

- **O erro que a metade de janela não pegou vai ao `seele.log`, em ERROR**, e
  cada caminho leva o que sabe:
  - a exceção no topo do código («o MOD lançou: …») e a promessa rejeitada sem
    tratamento («promessa rejeitada sem tratamento: …», dita no fim da volta e
    só se ninguém a pegou) levam o texto e a primeira linha da pilha, que diz
    onde;
  - a de um temporizador («erro num temporizador: …») e a de um ouvinte de
    evento («erro num ouvinte de evento: …») levam o texto e a pilha inteira,
    até o teto de 512 caracteres da linha.

  A pilha só vem quando o lançado é um `Error`. A gestão de MODs continua
  dizendo «falhou» como antes, na exceção do topo e na promessa.
- **O som do pacote toca por WebAudio**, sem `<audio>` e sem afrouxar a
  política da janela, e o contrato de `tocando` ficou escrito:
  - **`tocando` vale quando muda.** Um som que a pessoa pausou não volta
    sozinho no redesenho seguinte que o declare `tocando: true`.
  - **A exceção é o som que terminou:** `tocando: true` sobre um som cujo
    último aviso foi `terminou` toca de novo, do começo, como o `<audio>`
    fazia até a 0.15.0. Repetição explícita (`repetir`) é decisão da API 6.
  - **No fim, o MOD recebe só `terminou`.** O `<audio>` dizia `pausada`
    antes dele.
  - **O botão é do produto.** No lugar dos controles do `<audio>`, o produto
    desenha um TOCAR/PAUSAR ao lado do som (num cartão, nenhum).
  - **Fora da tela, a declaração não liga o som.** Um `tocando: true` num som
    cujo lugar saiu da tela — uma página fechada, um cartão que o SEELE não
    desenha agora — é recusado (`recusada`, com o motivo «o lugar deste som
    saiu da tela…»), e um som que tocava ali é pausado (`pausada`).
- **O som decodificado tem teto.** O WebAudio guarda o som inteiro em
  `float32`, e por isso há um teto além do de bytes: 64 MiB de som de pé por
  região, por superfície, por contribuição e nos cartões de um MOD — pouco
  menos de três minutos de som estéreo a 48 kHz, ou o dobro em mono. **E o MOD
  inteiro tem um teto, somando todos esses lugares: 128 MiB de som
  decodificado de pé** — pouco menos de seis minutos em estéreo, ou doze em
  mono. Acima de qualquer um dos dois, o som chega ao MOD como `recusada`, com
  os números no `porque`, como a mídia acima do teto de bytes, e o `seele.log`
  registra a recusa com o id do MOD. O que um lugar solta — a página que
  fecha, o som que sai da declaração — volta para a conta do MOD.

## Publicar a versão seguinte

O guarda trata como publicada toda versão até `MOD_API_VERSION`
(`crates/seele-proto/src/mods.rs`), e não só as que já saíram numa release: é
esse número que diz o que o build oferece. Por isso a ordem dos passos importa.

1. `api/vN.json` e a linha dela em `api/congeladas.sha256` entram no mesmo
   commit. Uma `vN.json` fora da lista reprova, e a reprovação dá a linha.
2. Enquanto `MOD_API_VERSION` for menor que N, a vN está em construção: a linha
   acompanha cada edição, e a reprovação dá a linha nova.
3. O `cargo xtask check-api` cobra `moments` e `eventos` desde que o arquivo
   existe, e não desde que a versão sai: a vN que não os escreve, ou que
   promete um nome sem despachante, reprova ainda em construção.
4. `MOD_API_VERSION` e `APIS_ACEITAS` sobem por último, quando a vN está pronta
   para sair, porque daí em diante ela está congelada, mesmo antes da release.
   Uma edição depois disso reprova como edição de versão publicada, e a mudança
   vai para a versão N+1. Subir antes faz de cada ajuste que a vN ainda pedir
   uma versão nova.

## Por que congelado

O [ADR 0045](../docs/adr/0045-mods-o-produto-base-tem-regras-e-um-mod-nao.md)
decidiu que a API de MOD é uma **fachada** e não uma projeção do protocolo. A
diferença é a que importa quando alguém renomeia um campo:

- projeção: o nome muda por dentro, a API muda junto, **todo MOD quebra**;
- fachada: o nome muda por dentro, o build fica vermelho, e o conserto é o
  **mapeamento** — o nome que o MOD escreve nunca mudou.

`cargo xtask check-api` é quem cobra isso, e a pergunta que ele faz é **«todo
nome deste arquivo ainda aponta para alguma coisa?»**. Nunca o contrário: um
guarda na outra direção arrastaria a API atrás do protocolo, e seria a projeção
de novo com outro nome.

É a mesma disciplina das migrações do servidor — *«Append only once shipped»*
mais `no_migration_contains_a_down_step` —, aplicada a um segundo lugar onde ela
vale pelo mesmo motivo.

## Quando congelar passa a doer

**Quando existe MOD de terceiro no mundo**, e não antes. Enquanto o indexador
não publica, este arquivo ainda se mexe. Depois, não: editar uma versão
publicada é quebrar o MOD de todo mundo, que é a razão pela qual esta porta não
fecha.

É isto que torna «liberdade total na v1» — o pedido do dono — construível sem
adivinhação: a superfície cresce enquanto ninguém depende dela, e fecha antes de
publicar em vez de antes de existir.

## A API 6

`api/v6.json` **congelou no corte** (15/10/2026; tarefa 13 do plano da F3):
`MOD_API_VERSION = 6` e `APIS_ACEITAS = [6]`, e a linha dela em
`congeladas.sha256` só muda por uma emenda da D-api-8, antes da publicação.
Depois que a 0.16 sair, o que vier vai para a v7. Até o corte ela esteve em
construção, com a linha acompanhando cada edição.

> **Emendado em 15/10/2026 — a primeira emenda depois do congelamento (D-api-8),
> antes da publicação.** A auditoria do `v6.json` contra o produto
> (`docs/superpowers/sdd/2026-10-08-0.16/F3/auditoria-do-v6/`) achou promessas
> que o produto não entregava, e a regra da D-api-8 é uma só: o `v6.json` diz
> o que o produto entrega, e nada além. A emenda corrigiu dez textos que
> descreviam outra coisa que o código faz, tirou o que não se entrega na 0.16
> (o `MessageEdited`, a folha expansível, a tira do palco e o «DE ONDE?» no
> celular, a pinça, a área mínima e o toque longo na tela), disse o teto real
> das mídias de uma superfície e o alcance da redução de mídia, e escreveu a
> lista fechada das recusas inteira, com a fronteira que a faz valer (o
> genérico `falhou`, as da ponte, as do `arquivo`, e a `forma-desconhecida`).
> A linha do v6 em `congeladas.sha256` foi refeita no mesmo commit.

> **Emendado em 16/10/2026 — a segunda emenda (D-api-8), antes da publicação.**
> Cada emenda é um commit só, e só escreve o que já foi entregue. Esta escreveu:
> - a `pessoa` da citação (D-api-9): `fundoDeMidia: { daPessoa, pessoa }` e
>   `retrato({ daPessoa, pessoa })` citam quem o MOD nomeia, numa superfície
>   também, se a pessoa estiver na sessão;
> - a errata do `sujo`: sem `inicial`, o inicial é o valor da **primeira**
>   montagem do campo, e um controle que o MOD regrava declara `inicial`. O
>   texto de antes induzia o defeito que a MESA teve;
> - o diálogo no celular: o modal na sessão acima do pé, com a faixa de voz e
>   as abas à vista, e o não modal embaixo, até 85% dela. O texto de antes
>   prometia as abas do MOD cabendo em 390 px, que é conteúdo do ESTILO, e não
>   entrega do produto (medido pela F8);
> - os nomes que vão na frente do par de uma recusa do tema: as cores de estado
>   (`fosforo`, `azul`, `vermelho`) e o segundo apagado (`apagadoSecundario`,
>   que saía como `apagado-secundario`; o achado do ESTILO);
> - o `teto` que a recusa `teto-de-alvos` passou a levar.
>
> A linha do v6 em `congeladas.sha256` foi refeita no mesmo commit.

> **Emendado em 16/10/2026 — a terceira emenda (D-api-8), antes da publicação,
> no mesmo commit da entrega.** Uma montagem com o que não coube ou não serve
> rejeitava com o genérico `falhou`, e quantos e por quê iam só ao `seele.log`.
> A emenda escreveu a recusa `nos-recusados`, que leva `recusados` (a conta) e
> `amostra` (até oito: o motivo, e a `chave`, a forma, a propriedade, o tipo ou
> a classe), no `montar`, no `atualizar` e no `classes` de uma superfície. E o
> evento `contribuicao` passou a levar a mesma amostra: uma forma que a API não
> tem conta em todo ponto (sumia calada fora do cartão), e o `classes`, que é de
> superfície, conta numa contribuição. A entrega e o texto vão juntos porque o
> guarda da lista fechada amarra a janela ao contrato nos dois sentidos.

> **Emendado em 20/10/2026 — a quarta e última emenda (D-api-10), no fim do
> passo 7 da auditoria.** Cada prova do passo 7 foi revertida contra o produto,
> e o que ele achou foi consertado no produto sempre que deu; esta emenda
> escreve só o que pedia texto. O `listarNaBarra` vale na aba SERVIDOR do
> celular (o texto ainda dizia a gaveta SALAS, de antes do rascunho §11). O
> `ponto-desconhecido` leva `pontoPedido` (o que veio, cortado em 64) e
> `pontos` (os que a API tem), entregues em `021876f8`. E `estilos` e `classes`,
> na lista da 6, são rótulos sem efeito (`compatibilidade.rotulos`): nenhum
> manifesto declara capacidade — ela vem com a `api` —, e nada pergunta por
> eles; o estilo vale onde a forma monta, e o `superficie-classes` pede
> `superficies`. Depois desta, a v6 só muda na API 7.

## A API 7 (0.17)

**A v6 saiu na v0.16.0 (`8faa3379b`) e está congelada de vez.** O `cargo xtask
check-api` também confere `congeladas.sha256` desde a 0.17, e o
`a_v6_da_arvore_e_a_publicada` prende o hash publicado.

**A 7 é um alargamento da 6** (E8 da 0.17; ADR 0063, e o ADR 0064 dos cargos):
a v6 inteira, mais o que os cargos pedem.

- **`manifesto.permissoes`** (`Manifest::permissoes`, `cargos::PermissaoDeclarada`):
  `[{ "acao": "fechar-mesa", "nome": "Fechar a mesa", "todos": false }]`.
  - Declarar exige `"api": 7`. Um manifesto da 6 com a chave é recusado
    com `PermissoesPedemApiNova`, e não ignorado.
  - A lista passa pela regra dos cargos (`validar_declaradas`): ação em
    `[a-z0-9-]` até 32 caracteres, nome não vazio, sem repetir, e o teto de
    permissões por MOD.
- **A ação `acao`** (`ClientMessage::AcaoDeMod`): o MOD pede uma ação
  declarada, e o servidor confere o MOD ligado, a ação declarada e a
  permissão da pessoa antes de chamar a metade de servidor. A recusa volta
  como `acao-nao-declarada` ou `sem-permissao`.
  - **Na janela, `SeeleMods.acao(id, canal, acao, valor)`**, só com a
    capacidade `acoes` (sem ela, o membro não existe). A mensagem da ponte é
    `acao`, e o id que vai é sempre o do próprio MOD, como no `request`. Ela
    devolve a resposta da metade de servidor, como o `request`, e a recusa dos
    cargos chega como valor: `{ ok: false, recusa: "sem-permissao" }`. As
    recusas da ponte são as do pedido (`too-many-requests`, `mensagem-grande`
    — o nome da ação conta no teto —, `sessao-encerrada`). A rota: o
    `mod_request` com `acao` (o comando Tauri e o `seele-web`) →
    `Connection::acao_de_mod` → `Enlace::acao_de_mod` → `Client::acao_de_mod`
    → `AcaoDeMod` no fio. A janela não confere a ação nem a permissão.
  - **O padrão que a metade de servidor segue.** O `ModRequest` comum
    continua valendo, e o servidor não o veta: as leituras do MOD seguem por
    ele. Quem guarda a operação protegida é o `aoPedir`, e só por
    `contexto.acao`, que o host escreve depois do portão dos cargos e que é
    `null` num `ModRequest`:

    ```js
    if (pedido.op === 'fechar' && contexto.acao !== 'fechar-mesa') {
      return JSON.stringify({ ok: false, erro: 'pede-a-acao-fechar-mesa' });
    }
    ```

    Nunca pelo corpo do pedido (um `pedido.acao`, um `admin` ou um
    `contexto` escritos pelo cliente não decidem nada), e nunca por
    `contexto.permissoes`: ela é informativa, para esconder botões, e chega
    também num `ModRequest` comum. Guardar a operação por ela deixaria o
    pedido legado executá-la sem passar pelo portão. O exemplo é a fixture
    `apps/seele-app/testes/mod-de-acoes/`, provada de ponta a ponta em
    `crates/seele-conformance/tests/acoes_de_mod.rs`.
- **A capacidade `acoes`**, a última da lista da 7 (`capacidades_da_api(7)`).

O que fica:

- **Um MOD da 6 roda na 0.17 sem permissões.** `APIS_ACEITAS = [7, 6]` e
  `APIS_POR_LINHA` é `("2", [7, 6])`, `("1", [6])`, `("0.15", [5, 4, 3])`.
  O que os cargos fazem com um MOD sem permissões declaradas é regra do ADR 0064.
- **A 0.16 não roda a 7, e a recusa dela não é a mesma nos dois casos**
  (medido contra o parser congelado dela, `crates/seele-proto/tests/manifesto_da_0_16.rs`):
  - um manifesto da 7 **com** `permissoes` é recusado como `Malformed`,
    porque a 0.16 tem `deny_unknown_fields` e não conhece a chave;
  - **sem** `permissoes`, é recusado como `ApiTooNew { 7, 6 }`.
  - Por isso o catálogo não pode oferecer a uma 0.16 uma versão da 7. A
    0.16 escolhe a versão pela `api` de cada entrada do catálogo
    (`ui/camada-mods.js`), e o indexador tem de escrever a `api` que o
    `mod.json` declara.
- **O mínimo de app e de linha:** a API 7 pede a linha `2` (a 0.17 em diante), e a
  6 roda nas linhas `1` e `2`.

**Até a 0.17 sair, a v7 segue a regra da v6 antes da publicação** (D-api-8).
`MOD_API_VERSION` já é 7, então ela conta como oferecida. Cada emenda é um
commit só, escreve só o que foi entregue e refaz a linha dela em
`congeladas.sha256`, com a razão escrita lá. Depois da 0.17, a mudança vai
para a v8.

O que muda na forma do arquivo da v6, e vale também para a v7:

- **a v6 é escrita por inteiro**, sem `extends`: só a API 6 roda no app 0.16,
  e herdar traria de volta o que as versões 1 a 5 prometeram e não entregaram;
- **todo apontador é um símbolo do código** (`nome` ou `Tipo::membro`), e o
  que ele quer dizer, quando precisa de frase, está no bloco **`sentido`**, sob
  o mesmo nome. O `cargo xtask check-api` reprova, a partir da 6, o apontador
  que é prosa: ele confere o texto depois do último `::`, e uma frase passava
  por qualquer sufixo que existisse nas fontes;
- **os tetos que o produto aplica são números**, conferidos contra as
  constantes da casca: `contribuicoes.teto` (as contribuições sem alvo de pé,
  por MOD), `contribuicoes.alvos-por-ponto` e cada `superficies.limites`.

**O `ok` de `mundo.tema` quer dizer «a forma é válida e o tema foi publicado»,
e não «aplicado».** O servidor confere a forma — as chaves, os valores, o
`modo`, as regiões — e publica; os pares de contraste são de cada janela, que
conhece a paleta do produto e as cores que deriva dela, e cada uma conta ao MOD
pelo evento `tema`: `aplicado`, ou `recusado` com os pares e a região.

## Os quatro blocos

- **`reads`** — o que um MOD lê do domínio.
- **`moments`** — quando ele é chamado. São eventos do `ServerMessage`, com os
  nomes que o fio já usa: um relatório de defeito que diz `MessageReceived`
  acha o mesmo nome no protocolo, sem intermediário. **Da v1 à v5, só 5 dos 21
  momentos são entregues.** A v1 lista 21, a v2 à v5 os herdam pelo `extends`,
  e `momento_de` (`crates/seele-server/src/mods/despacho.rs`), o único
  despachante de momentos do servidor, tem braço só para `PersonJoined`,
  `PersonLeft`, `MessageReceived`, `MessageEdited` e `MessageRemoved` — e o
  `MessageEdited` nem esse entrega, porque nada o emite: o produto não edita
  mensagem, e a v6 não o lista (a emenda de 15/10). Os outros 16 nunca
  foram entregues: um MOD que espera por `ChannelCreated` espera para sempre,
  sem erro. A v1 não se edita, e por isso a v6 lista só o que é entregue: da
  API 6 em diante, `moments` e `eventos` são escritos por inteiro em cada versão
  e não se herdam nem se somam pelo `extends`, e o que a versão lista é tudo o
  que ela promete. O `cargo xtask check-api` reprova a versão que não os escreve
  e a que lista um nome sem despachante.
- **`actions`** — o que ele manda o servidor fazer. São verbos do
  `ClientMessage`, e passam pelas **mesmas permissões** que a janela atravessa:
  não há caminho paralelo, então não há semântica paralela para divergir.
- **`own`** — o quintal do MOD, e ele tem duas metades: chave→valor no banco do
  servidor, e **arquivos numa pasta que é dele**, em `mods/<autor>/<nome>/dados/`.
- **`world`** — o que não está no protocolo e não é do MOD: **rede de saída**,
  relógio e registro.

### O único freio da «liberdade total», e por que ele existe

O pedido do dono foi liberdade total desde a v1, e é o que está construído em
todo o resto. **O disco é a exceção, e ela é de uma linha:** um MOD lê e escreve
na pasta dele, e não na máquina.

O motivo não é gosto. É o que fica ao lado da pasta `mods/`, hoje:

```
identity.key   conhecidos   pins   seele.db   anexos   seele.log
```

Disco inteiro significaria **a chave privada de identidade**, os **pinos TOFU** e
o **banco com todas as conversas** — e aí o que cai não é uma regra de produto,
é o ADR 0004 e o 0017, que são o que garante que a pessoa é ela mesma. «As
regras do produto não alcançam MODs» é uma coisa; entregar a identidade de quem
hospeda junto é outra.

E rede e disco não são o mesmo grau, que é o que torna esta linha desenhável:
**rede deixa um MOD mandar para fora o que ele já enxerga; disco decide o que ele
enxerga.** Sozinha, a rede não alcança o `identity.key`.

**O que a pasta própria entrega mesmo assim:** volume de verdade — um MOD com
milhares de imagens não cabe num KV com teto —, importar e exportar, e uma pasta
que quem hospeda abre no Finder para ver o que o MOD guardou.

**A saída, para o caso que ela não cobre:** falar com outro programa da máquina
— OBS, um jogo, um bot que já roda ali — é o único uso que pede o disco inteiro,
e ele entra, se entrar, como **capacidade declarada no manifesto e mostrada em
separado na tela de aceite**, pelo caminho que o `reach` já foi desenhado para
fazer. Quem instala lê «este MOD lê o seu disco inteiro» como uma linha própria,
e não escondida dentro de «este MOD roda no servidor».

**O escopo é conferido pelo mesmo código que o `mod://` usa** —
`inner_path`, no `seele-proto`, que reconstrói o caminho por componentes e recusa `..` em vez
de resolver. Aquela função ganhou testes próprios depois de a prova por reversão
mostrar que ela não guardava nada; é por isso que ela é o alvo do mapeamento
aqui, e não uma segunda cópia da mesma ideia.

## O lado direito não é um caminho de código

O valor de cada nome é o **símbolo** que o `check-api` procura no repositório.
Ele é textual de propósito: um resolvedor de verdade exigiria o compilador, e o
que este guarda tem de pegar — um rename — muda o texto. O custo é que ele acha
o símbolo em qualquer lugar do repositório, e não exatamente onde o mapeamento
diz. É um guarda contra desaparecimento, e não contra mudança de lugar.
