# ADR 0044 — MODs: o produto base tem regras, e um MOD não

**Estado:** proposto
**Data:** 2026-09-05

**Substitui o [ADR 0029](0029-mods-declaram-valores-e-o-produto-mede.md)** por
inteiro, e revoga partes nomeadas dos ADRs 0022, 0026 e 0032 e de
`specs/00` e `specs/07`. A tabela de revogações está no fim da Decisão.

O desenho longo, com a ordem de entrega e o que cada exemplo de MOD exercita,
está em `docs/superpowers/specs/2026-09-05-mods-design.md`. Esta página é a
decisão.

## Contexto

Havia uma decisão escrita sobre isto, e ela era quase o contrário desta. O **ADR
0029**, de 17/08, também nasceu de um pedido do dono, também previa indexador,
MODs oficiais e código aberto — e decidiu que **um MOD é um arquivo de valores
que nunca traz código**, que ele é da máquina de quem instala, e que **um MOD
não acompanha um servidor**. O 0029 continua `proposto` e **nada dele foi
construído**, o que torna esta substituição barata em código e cara só em página.

O pedido de hoje é diferente em três eixos, e a diferença é do dono:

- **O MOD é do servidor.** *«Esses MODs são por SERVIDOR. Os dados são
  persistidos no server, e quem entrar no server precisa baixar esses MODs.»*
- **Ele muda as duas coisas** — o comportamento da sala e a aparência da janela
  de quem entra.
- **E a frase que decidiu tudo:** *«o que construímos até agora tem regras. Mas
  essas regras não se aplicam aos MODs, e essa é a graça.»* Com a analogia junto:
  Minecraft — jogo base com mecânicas, e MODs que as complementam ou **as
  refazem de outro jeito**.

O 0029 foi construído inteiro em cima da premissa oposta: *«toda defesa daqui
repousa numa pessoa decidindo»*. Removida essa premissa, não sobra nada dele de
pé, e emendá-lo seria pior que substituí-lo.

### O que existe hoje, e que condiciona o desenho

Conferido no código antes de qualquer linha desta página.

- **A janela é embutida no binário e a CSP não tem folga.**
  `frontendDist: "./ui"` com `tauri::generate_context!()`, e a linha inteira em
  `apps/seele-app/tauri.conf.json:23`: `script-src 'self'`, sem `unsafe-inline`,
  sem `unsafe-eval`, sem origem remota. **Não existe hoje caminho por onde um
  `.js` de terceiro entre na página.**
- **A casca é uma só, e é JavaScript.** `apps/seele-app/ui/` são ~15 mil linhas
  sem framework por decisão (ADR 0019); `tela-sessao.js` sozinho tem 3.297. A TUI
  saiu no ADR 0039, então não há mais duas cascas a satisfazer.
- **A janela fala com o motor por `invoke()` e `listen()`**, e já há ~40 comandos
  em `apps/seele-app/src/main.rs`.
- **O servidor é um SFU com orçamento declarado.** `xtask/src/check_deps.rs`
  proíbe `seele-server` de depender de `seele-core` e de `seele-audio`, e escreve
  o motivo: *«the server is an SFU… supposed to fit in 1 vCPU / 512 MB»*. É um
  build vermelho, não uma recomendação.
- **As migrações são append-only e não há reversa.** `persistence/schema.rs` diz
  *«Append only once shipped»* e `no_migration_contains_a_down_step` reprova
  qualquer `DROP`. Há **dez** migrações; a próxima é a 11.
- **Nome e ícone de servidor já atravessam o fio**, com
  `Permission::AdministerServer` — `RenameServer`, `SetServerIcon`,
  `ServerIconChanged` —, e o ícone é **PNG e só PNG, 8 KiB, lado ≤ 256 px**, com
  GIF recusado por construção porque *«animaria numa placa»* (adendo do ADR
  0032, 22/08).
- **O produto não tem licença.** `README.md`: a postura de direitos continua em
  aberto. Isso importa para a exigência de código aberto e está tratado abaixo.

## Decisão

**Um MOD é código de verdade, escrito por terceiros, rodando nos dois lados, com
acesso real. Não há caixa de areia. A defesa é código aberto obrigatório mais
avaliação de cada versão publicada — e isso fica escrito como o que é: a única
defesa real.**

*O «revisão nossa» com que esta frase foi escrita virou «avaliação automática,
com humano no que ela não fecha limpo» no adendo de 06/09. O que a frase diz
sobre ser a única defesa continua valendo, e é por isso que ela não foi
reescrita: o que mudou foi quem revisa, não que a revisão seja tudo o que há.*

### «Sem caixa de areia» é a decisão inteira

Tudo o mais aqui é consequência, então vale gastar o parágrafo.

O ADR 0029 recusou código de verdade com o argumento de que *«caixa de areia
limita capacidade e não julga intenção»* e de que **todas** as garantias deste
repositório cairiam de uma vez. Os dois continuam verdadeiros. O que mudou é
quem os assume: **as garantias protegem o produto base, e um MOD está fora delas
por decisão de quem é dono do produto.**

Isso não é um afrouxamento acidental, e por isso o alcance é escrito por extenso
em vez de deixado por omissão:

- Um MOD de cliente **alcança a janela inteira**. Não há isolamento de contexto,
  não há iframe, não há ponte estreita.
- Um MOD **anima livremente**. `specs/07` diz que movimento é diagnóstico; a
  regra segue valendo para o produto e **não alcança MOD**.
- Um MOD **pinta o que quiser**. Os quatro guardas do vermelho em `frontend.rs`
  continuam cobrando **as nossas folhas**, e não têm o que dizer sobre um MOD.
- Um MOD **abre rede a partir do servidor**, inclusive túnel com retransmissão de
  terceiro — o que o ADR 0022 pôs fora de escopo no degrau 5.

O que este ADR **não** faz é fingir que existe uma defesa técnica. Ela não
existe. Existe uma defesa humana, e ela está na seção do indexador.

### JavaScript nos dois lados, e o motivo não é elegância

1. **Fonte é artefato.** Não há build para reproduzir, então a revisão lê
   exatamente o que roda. Com artefato compilado, revisaríamos o repositório e a
   pessoa instalaria um binário — **a revisão viraria teatro**, e ela é a única
   defesa que sobrou. Isto também mantém o indexador servindo arquivo estático em
   vez de virar serviço de build.
2. **É a exigência de código aberto ficando grátis e absoluta.** É o mesmo
   argumento que o 0029 usou para o MOD de cor — *«publicar o fonte não é uma
   promessa sobre o arquivo: é o arquivo»* — e ele sobrevive intacto à mudança de
   tudo o mais.
3. **A casca já é JS.** A metade de cliente de um MOD é trabalho de DOM.
4. **Um artefato, três sistemas.** Ninguém compila nada.
5. **O piso de entrada decide o tamanho do catálogo.** O ecossistema do Minecraft
   existe porque o piso era Java. Exigir Rust não deixaria o desenho mais puro:
   deixaria o catálogo vazio, e um indexador com cinco MODs é um indexador
   fracassado.

**O que se perde, dito aqui:** não há checagem de tipo no lado do servidor. Um
erro de digitação chega em execução, na máquina de quem hospeda. A contrapartida
obrigatória é a seção «Falha isolada».

**Qual interpretador embutir foi medido, e o número está em
`spikes/mod-em-js/`.** Era a única peça deste ADR que dependia de um número que
ninguém tinha — tamanho de binário, memória por contexto, custo de chamada,
contra 1 vCPU / 512 MB. Duas candidatas, QuickJS pelo `rquickjs` e Boa em puro
Rust:

| | QuickJS | Boa |
|---|---|---|
| delta de binário | **1 098 KiB** | 12 088 KiB |
| custo por chamada | **39 ns** | 73 ns |
| RSS com 50 contextos | **5,9 MB** | 24,8 MB |
| licença | MIT | Unlicense OR MIT |

**QuickJS, e não por pouco:** 11× menor, ~2× mais rápido, ~4× menos memória. E a
intuição que eu trouxe para esta página estava errada — supunha-se que um runtime
WASM seria a opção leve e o interpretador JS o peso; é o contrário, e 1 MiB cabe
folgado nos 512 MB. `CLAUDE.md`: *«Três vezes uma hipótese confiante custou mais
que a medida teria custado.»*

Os dois tetos que esta página promete **existem e foram conferidos**:
`set_memory_limit` corta um MOD que aloca sem parar, `set_interrupt_handler`
corta um `while (true) {}` — e, nos dois casos, **o contexto sobrevive**, que é a
metade que a seção «Falha isolada» precisa.

**O custo, dito:** `rquickjs-sys` compila fonte em C, então o build passa a
exigir um compilador de C nos três alvos. Ele já existe nos três; a exigência é
nova para um projeto que até aqui compilava só com o Rust, e 11 MiB a mais em
cada instalador é caro demais para evitá-la.

### A API é uma fachada congelada, não uma projeção do protocolo

Esta seção existe porque a primeira versão dela estava errada e o dono achou o
erro. Se a API fosse **gerada a partir de `seele-proto`**, renomear um campo lá
dentro repintaria a API e quebraria todo MOD publicado.

**O guarda é invertido.** Ele não pergunta «todo tipo do protocolo chegou à
API?». Ele pergunta **«todo nome da API ainda aponta para alguma coisa?»**.

```
api/v1.json    "pessoa.papel"     ←  Person::role
               "pessoa.apelido"   ←  Person::nickname
```

- **Renomear por dentro fica livre.** O build fica vermelho e o conserto é o
  mapeamento, não os MODs. O nome que o MOD escreve nunca mudou.
- **Acrescentar ao protocolo não mexe na API.** Um tipo novo só aparece para MODs
  quando alguém decide, e essa decisão sobe a versão da API. Nada vaza por
  acidente.
- **Cada versão da API é um arquivo congelado**, e um teste reprova qualquer
  edição num arquivo já publicado.

A disciplina não é inventada aqui: é a das migrações — *«Append only once
shipped»* mais `no_migration_contains_a_down_step` —, que hoje segura dez
migrações, aplicada a um segundo lugar onde ela vale pelo mesmo motivo. É também
a mesma tese de `check_deps.rs`: *«um contrato conferido só por revisão é um
contrato que se corrói.»*

**Um MOD declara uma versão de API, uma só.** Para valer numa versão nova, o
autor publica de novo — passando pela revisão outra vez. Quando MOD e servidor
discordam, **o MOD não carrega e diz**: o servidor sobe sem ele, e quem tem
`AdministerServer` lê qual MOD, que API ele pede e qual o servidor oferece. Isto
é a única coisa que o ADR 0029 lega inteira a esta página: **instalação parcial
silenciosa é o modo de falha a evitar.**

**O que a fachada não salva:** nome é barato de preservar, comportamento não é.
Se «expulsar» passar a significar outra coisa, nenhuma fachada segura um MOD que
dependia do significado antigo. É para esse dia que o **ADR 0045** existe.

### O único freio da liberdade total, e ele é de uma linha

Um MOD alcança tudo — inclusive rede de saída, que é o que o MOD de túnel do
dono exige. **A exceção é o disco: um MOD lê e escreve na pasta dele**,
`mods/<autor>/<nome>/dados/`, e não na máquina.

O motivo não é uma regra de produto se aplicando a MOD. É o que fica ao lado
daquela pasta, no diretório do ADR 0017: `identity.key`, `pins`, `conhecidos` e
o banco com todas as conversas. Disco inteiro entregaria a **chave privada de
identidade** de quem hospeda, e aí o que cai não é estética: é o ADR 0004 e o
0017, que são o que garante que a pessoa é ela mesma.

**Rede e disco não são o mesmo grau, e é isso que torna a linha desenhável:**
rede deixa um MOD mandar para fora o que ele já enxerga; disco decide o que ele
enxerga. Sozinha, a rede não alcança o `identity.key`.

A pasta própria entrega os três usos que existem — volume de verdade, importar e
exportar, e um lugar que quem hospeda abre para ver o que o MOD guardou. O
quarto — **falar com outro programa da máquina** — é o único que pede o disco
inteiro, e a saída está nomeada: capacidade declarada no manifesto e mostrada
**em separado** na tela de aceite, pelo caminho que o `reach` já tem. Quem
instala lê «este MOD lê o seu disco inteiro» como uma linha própria.

O escopo é conferido por `seele_proto::mods::inner_path` — a mesma função que o `mod://` usa,
e não uma segunda cópia dela.

### Como o código entra na janela

`script-src 'self'` recusa qualquer `.js` que não esteja embutido, e afrouxar
para `unsafe-eval` seria abrir a porta mais larga do prédio para resolver o
problema mais estreito.

**A saída é um protocolo próprio do Tauri.** Os arquivos do MOD ficam em disco,
um handler os serve sob `mod://`, e a CSP ganha esse esquema — e só ele — em
`script-src`. O carregamento continua sendo o navegador buscando um arquivo, e
**nada é servido sob `mod://` cujo hash não bata com o que o servidor
declarou**.

`the_page_loads_only_files_that_are_shipped` (`frontend.rs:520`) ganha uma emenda
de **escopo, não de rigor**: ele passa a valer sobre `self`, e `mod://` fica com o
guarda de hash acima.

### Onde mora, quem administra, e o que acontece ao remover

**Migração 11:** uma tabela de MODs — identidade, versão, hash, habilitado,
configuração — e um espaço de dados nomeado por MOD, com teto declarado.

**Habilitar e desabilitar vão pelo fio, com `Permission::AdministerServer`** — a
mesma permissão de `RenameServer` e de `SetServerIcon`, semeada só no Comandante
pela migração 1. Isto contraria a primeira intuição, que era administrar só pela
máquina que hospeda, e o motivo está no adendo do ADR 0032: aquele argumento é
sólido e **responde outra pergunta**, porque *«deixa um servidor rodando por
`seeled` numa VPS sem forma nenhuma de ser nomeado por quem o administra»*.

**Desabilitar preserva os dados; remover apaga, avisando o que vai embora.**
Desabilitar tem de ser barato e reversível, ou ninguém desabilita para testar.

Um MOD que sobe de versão **traz a migração dele**, append-only pela mesma regra
das nossas. Um MOD que não sabe migrar o próprio dado não atualiza.

### Quem entra, e o que ele aceita

- Na **primeira vez** que entra num servidor com MODs, a pessoa vê nome, autor,
  versão, repositório e o **alcance declarado** de cada um — em particular se o
  MOD lê o que ela escreve.
- Aceitou uma vez, entra direto depois. **Um servidor que troca de MOD pergunta
  de novo.**
- **Quem não aceita, não entra.** O MOD é parte do que aquele servidor é.

### Os bytes vêm do indexador, e isso tranca a rede local

Decidido depois de o custo ser mostrado, e por isso registrado como decisão e não
como descoberta:

> **Um servidor em rede local, sem internet, com um MOD habilitado, tranca todo
> mundo do lado de fora — inclusive quem hospeda.**

O cliente bate em `mods.seele.app.br` e confere o hash; um MOD só existe se
estiver publicado. **Não há cache no servidor, não há espelho e não há caminho
lateral** — foi proposto servir os bytes pelo próprio servidor, com a assinatura
garantindo a procedência do mesmo jeito, e foi recusado: quem entra tem de bater
na origem.

Isto contraria o ADR 0022, que chama o ponto de encontro de *«opcional, trocável
e não obrigatório»*, e o 0029, que escrevia que *«nada em um MOD depende de ele
ter vindo de lá»*. As duas frases deixam de valer para MODs.

A tela de hospedar tem de dizer isso **no momento de habilitar o primeiro MOD**,
não depois.

### Falha isolada

**Um MOD que lança exceção é desabilitado, e a sala continua.** Quem tem
`AdministerServer` vê qual, quando e a linha. Vale para as duas metades: um MOD
de cliente que quebra não pode levar a janela junto.

Não é polidez — é a contrapartida de escolher JavaScript. Sem isolamento de
falha, um MOD ruim é uma queda de servidor, e o defeito que o `CLAUDE.md` nomeia
como o mais caro deste repositório é *«o produto sabe e não conta»*.

### O indexador, e a defesa que ele é

**É nosso**, e é a primeira coisa que este projeto hospeda além do release no
GitHub.

- **Repositório público obrigatório**, como requisito de publicação e não como
  campo declarado.
- **Revisão de código de cada versão.** Não só da primeira: um MOD limpo
  aprovado cuja versão seguinte entra sozinha é uma revisão que não significa
  nada.
- **Endereço fixo: `mods.seele.app.br`.** O cliente bate na origem, e não num
  espelho. Isto **diverge do ADR 0029**, que previa endereço configurável e
  espelhamento — *«quem não quiser que sejamos nós a servir os bytes serve os
  mesmos bytes de outro lado»* —, e a divergência é decisão do dono. O custo
  está em «O que fica sem saída».
- **Catálogo assinado, busca no cliente.** Não é uma API. Com API, o indexador
  aprende cada termo digitado; com catálogo, aprende que alguém buscou o
  catálogo.
- **Sem consulta automática ao abrir** — mesma regra do ADR 0026.
- **MOD oficial é assinatura conferida offline**, com chave **separada** da do
  atualizador: uma chave que atesta duas coisas deixa as duas se passarem uma
  pela outra, e a do 0026 autoriza instalar programa.
- **Remoção alcança quem já baixou**, e é a mesma peça da revogação de versões do
  [ADR 0045](0045-toda-versao-continua-de-pe.md).

### O que este ADR revoga

| decisão | onde | o que acontece |
|---|---|---|
| «Um MOD não acompanha um servidor» | 0029 | revogada |
| «Nada de código arbitrário» | 0029 | revogada |
| Lista fechada de capacidades | 0029 | revogada |
| «Nenhuma capacidade dá a um MOD lugar para escrever» | 0029 | revogada |
| Indexador sem revisão | 0029 | emendada — revisão de cada versão |
| Cor da placa por lista fechada | 0032 | absorvida; não se constrói separada |
| «Um servidor não repinta a janela de quem entra» | 0032 | revogada |
| GIF recusado por construção | 0032, adendo | vale para o produto, não para MOD |
| Retransmissão de terceiro fora de escopo | 0022, degrau 5 | permitida por MOD, com aviso nomeando o terceiro |
| «Movimento é diagnóstico», exceção única | `specs/07` | vale para o produto, não para MOD |
| `marketplace de plugins` como não-objetivo | `specs/00` | desfeita |

## Alternativas consideradas

1. **O ADR 0029 como está: valores declarados, sem código.** Mantém todas as
   garantias sem uma linha de teste mudar, e é a alternativa mais segura desta
   lista. Recusada porque não faz nenhum dos seis MODs que o dono descreveu — um
   esquema declarativo não faz um bot de chat, não faz salas de RPG e não abre
   túnel. Para cobrir esses casos o esquema precisaria de condicional, aritmética,
   aleatório e leitura do próprio estado, e nesse ponto **ele é uma linguagem de
   programação, só que ruim**: sem tipos, sem depurador, sem mensagem de erro, e
   cada ideia nova de um autor virando uma versão de esquema que só nós podemos
   publicar.

2. **Código, mas em caixa de areia com API fechada.** Foi a recomendação inicial
   desta conversa e foi recusada pelo dono, com a razão explícita de que as regras
   do produto não devem alcançar MODs. Registro o que ela comprava, porque o dia
   de reabrir isto pode chegar: a máquina de quem **entra** nunca executaria
   código de terceiro, e os guardas do vermelho continuariam significando alguma
   coisa do lado do cliente — que é onde está a pessoa que não escolheu o MOD.

3. **WASM em vez de JavaScript.** Desempenho previsível e o autor escolhe a
   linguagem. Recusada porque o artefato passa a ser compilado, e aí ou o
   indexador vira serviço de build ou **a revisão de código revisa uma coisa e a
   pessoa instala outra**. Some-se o piso de entrada, que decide o tamanho do
   catálogo.

4. **Plugin nativo, como o Forge de verdade.** Poder total e zero cerimônia. Em
   Rust não há ABI estável: cada versão do SEELE quebraria todo MOD, e cada MOD
   precisaria de build para três sistemas. A recusa é mecânica, não de política.

5. **Nada — quem quiser que faça fork.** É de graça e é o que acontece na
   ausência desta página. Recusada pelas três razões do 0029, e a que mais pesa
   continua sendo a terceira: um fork tira a pessoa do caminho do binário
   assinado, num produto que já apanha do SmartScreen e do Gatekeeper.

## Consequências

- **O ADR 0029 é substituído**, e o 0032 perde a seção da cor, que nunca foi
  construída. O não-objetivo de `specs/00` cai, e a lista de
  `the_settings_screen_omits_what_the_product_lacks_instead_of_drawing_it_dead`
  perde `TEMA`.
- **A CSP muda** — pela primeira vez desde que foi escrita — para admitir
  `mod://` em `script-src`. Se fosse preciso mais que isso, o desenho estaria
  errado.
- **A migração 11 chega**, e o banco do servidor passa a guardar dado de
  terceiro. É a primeira vez.
- **Uma dependência grande entra na árvore do servidor**, e ela é a maior que
  este projeto já engoliu. Precisa de licença compatível com o `deny.toml`, que
  nega GPL/AGPL/LGPL por omissão, e de medida contra 1 vCPU / 512 MB.
- **A revisão de código vira trabalho recorrente de gente**, para sempre e
  linearmente com o catálogo.
- **Uma terceira chave para guardar** — depois da do sistema operacional e da do
  atualizador —, com o mesmo problema de custódia que a pendência 16 já mostrou
  não ser trabalho de código.
- **`specs/07` deixa de ser sobre a tela e passa a ser sobre a nossa tela.** É uma
  mudança de estatuto, não de texto.

## O que fica sem saída

**A revisão de código é a única defesa.** *(Emendado pelo adendo de 06/09: ela
passa a ser por máquina, com humano no que não fecha limpo. O que continua
verdade é o resto deste parágrafo.)* Um MOD malicioso que
passe pela leitura roda com acesso real na máquina de quem hospeda e na de todo
mundo que entrou. A profundidade da defesa é a atenção de quem revisa, num dia
qualquer, no vigésimo MOD da semana.

**Ler o fonte não é entender o fonte.** JavaScript ofuscado é fácil de recusar. O
que não é fácil é código limpo que faz uma coisa a mais, discreta, na décima
função.

**O indexador aprende quem entra em qual servidor com MOD, e quando.** O ADR
0029 amortecia isto com duas palavras — espelhável e opcional — e as duas caíram
com o endereço fixo. Não é conteúdo e não é conversa; é o degrau 4 do ADR 0022
sem as reduções que aquela página tinha comprado.

**A rede local sem internet não funciona quando há MOD.** Um produto cujo
argumento é «o servidor é seu» passa a ter um caso em que entrar numa sala depende
de `mods.seele.app.br` estar no ar. Foi apresentado duas vezes, com a alternativa
de o servidor servir os bytes que já baixou, e reafirmado as duas: **quem entra
bate na origem.** Se aquele domínio cair, ninguém entra em servidor com MOD.

**Um MOD pode ser feio, lento e ruim, e passar em tudo.** O produto pode tornar a
leitura possível e não pode torná-la provável. A resposta que resta é a mesma de
sempre — a pessoa desabilita —, e ela só funciona porque ela consegue.

**Exigimos de terceiros uma coisa que o produto ainda não fez.** O SEELE não tem
licença. Por isso o requisito é **repositório público**, que é verificável, e não
uma licença nomeada, que seria cobrar uma promessa que não fizemos.

## Custo de reverter

**Baixo antes do primeiro MOD publicado.** Um manifesto, uma migração aditiva, um
handler de protocolo, uma tela e um diretório. A CSP volta uma linha.

**Alto depois, e num lugar específico:** a API congelada não pode ser editada por
construção. Ela é o produto, não uma implementação dele — quebrá-la é quebrar MOD
de terceiro, que é a razão pela qual essa porta não fecha. É a mesma advertência
que o ADR 0017 escreveu sobre o formato do `identity.key`.

## Adendo — a avaliação passa a ser por máquina, e o veredito tem três alturas (2026-09-06)

O corpo acima diz que a defesa é **«revisão nossa de cada versão publicada»** e a
chama de *a única defesa real*. Isso muda aqui, e a frase muda junto — porque
uma defesa descrita errado é pior que uma defesa fraca.

Pedido do dono, nas palavras dele: o autor clona um repositório base, publica no
**repositório dele**, e **pelo site** pede inclusão passando a URL. A avaliação
clona e analisa **o repositório inteiro** — estrutura, e o código contra código
malicioso, comunicação com software de terceiro e tentativa de invasão. Aprovado,
recebe verificado. Não aprovado, depende do que ele faz.

### O que a avaliação por máquina é, dito com precisão

**Um filtro, e não uma prova.** Ela pega o óbvio — `eval` de string remota,
exfiltração escancarada, ofuscação — e não pega o caminho sutil na décima função
de um arquivo limpo. Não pega intenção, que é exatamente o que a alternativa 2
deste ADR dizia sobre caixa de areia.

Ela resolve um problema que estava escrito em «O que fica sem saída»: *«a revisão
não escala, e isso é um fato e não um risco»*. Trocar humano por máquina troca um
gargalo por um teto de qualidade, e é uma troca legítima — desde que o teto seja
dito.

**Então a frase do corpo passa a ser:** a defesa é **repositório público
obrigatório mais avaliação automática de cada versão, com olho humano em tudo o
que ela não fecha limpo.** O terceiro nível abaixo é precisamente um caso que
exige o olho humano, então ele não é opcional no desenho.

### Os três níveis

| | o que é | como é provado |
|---|---|---|
| **oficial** | nosso, e nós respondemos por ele | assinatura com a chave de MOD |
| **verificado** | passou na avaliação | **assinatura**, e não campo |
| **publicado com notas** | não passou limpo, e o que faz é legítimo — integração com terceiro conhecido, por exemplo | assinatura **que cobre as notas** |
| *negado* | lesa quem instala | não entra |

**«Verificado» é assinatura e nunca campo**, pelo argumento que o ADR 0026 já
escreveu na alternativa 5: um catálogo adulterado forja um campo e não forja uma
assinatura. **E as notas de segurança do terceiro nível ficam dentro do que é
assinado** — notas que alguém pode tirar são notas que não protegem ninguém.

O terceiro nível é a parte nova mais valiosa deste adendo: sem ele, «não passou»
só teria a forma de «negado», e todo MOD que conversa com qualquer coisa de fora
seria recusado. Com ele, a pessoa que instala lê o que o MOD faz de incomum e
decide — que é a frase que este ADR usa desde o começo, agora com um mecanismo
atrás.

### O commit é fixado, ou a avaliação não vale nada

O catálogo carrega o **hash do commit avaliado**, e os bytes servidos saem dele.
Sem isso o autor dá `push` depois da aprovação e passa a distribuir outra coisa
sob o mesmo veredito — o mesmo teatro que este ADR já recusou para artefato
compilado, voltando por outra porta.

Um `push` depois da aprovação não muda o que ninguém baixa. Ele exige
solicitação nova.

### O que o site pode e não pode

O formulário de solicitação é uma **escrita**, e o [indexador](../indexador-de-mods.md)
é hospedagem estática de propósito. A separação que vale, e que precisa ficar
escrita para ninguém a desfazer por conveniência:

- **a submissão ganha caminho de escrita** — um formulário que abre uma issue, ou
  uma função de borda que faz o mesmo;
- **o catálogo continua parado.** Nenhuma rota de busca, nunca. É a propriedade
  inteira: com API o indexador aprende cada termo digitado, e com arquivo ele
  aprende que alguém buscou o arquivo.

No dia em que existir um Worker para a submissão, alguém vai propor usá-lo para
a busca. A resposta está aqui.

### E a conferência de assinatura no navegador não é controle de segurança

O site baixa o catálogo, a assinatura e a chave **da mesma origem**. Quem
adultera o catálogo adultera o verificador junto, e a chave junto — não há
âncora. A conferência do cliente vale porque a chave está **compilada no app**, e
quem controla a CDN não a alcança; o site não tem equivalente.

Ela fica, porque pega corrupção acidental e CDN mal configurada. O que não pode
é a tela prometer o que ela não dá: a palavra é **íntegro**, e não **autêntico**,
e a página diz uma vez que a conferência que decide acontece no app.

Consequência prática: **assinar em modo legado para o navegador poder conferir
não se justifica por isso.** Justifica-se por o catálogo ser pequeno — o
pré-hash existe para arquivo grande —, e a razão certa é a que fica escrita.
