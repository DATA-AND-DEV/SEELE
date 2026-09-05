# ADR 0027 — Anexos: o Dogma guarda, com teto, e o mais velho sai

**Estado:** aceito
**Data:** 2026-08-17
**Construído:** 2026-08-17

> **Vocabulário.** Esta página é anterior ao [ADR 0035](0035-o-codigo-deixa-de-falar-evangelion.md)
> e diz `Dogma` onde o produto hoje diz **servidor**, `Cage` onde diz **sala de
> voz** e `Linha` onde diz **canal**. O texto fica como foi escrito — o 0035
> preserva o registro de ontem de propósito. Os dois caminhos de arquivo que
> apontavam para `src/casper/` foram corrigidos para `src/persistence/`, porque
> aqueles não são registro: são endereços, e um endereço errado manda a próxima
> pessoa para uma pasta que não existe.

Tudo o que este documento decide está de pé. Duas coisas que ele descreve não
tinham sido construídas, e as duas foram construídas depois. Ficam aqui, com a
data, em vez de sumirem do texto:

- **A prévia embutida de imagem.** A regra escrita abaixo — desenhar só uma
  lista curta de tipos de imagem, e só quando os bytes concordam com a alegação
  — exige baixar os bytes e olhar os primeiros deles antes de escolher um
  decodificador. Enquanto essa conferência não existir, **todo** anexo é o outro
  ramo da mesma regra: um arquivo com nome e tamanho, sem prévia. É o lado
  seguro, e não é o lado completo.

  **Isto foi construído em 2026-08-18**, pelo mesmo relato de campo que trouxe
  o seletor: *preview de imagem/documento anexo*. A conferência existe, são
  quatro formatos com o motivo de cada um escrito, e a seção «O que os
  primeiros bytes decidem» conta o que entrou, o que ficou de fora, e o que
  acontece quando os bytes discordam do nome. A regra não foi pulada: ela foi
  construída.
- **Um seletor de arquivos nativo.** Escolher um arquivo no app é arrastá-lo
  para a conversa, e salvar grava na pasta de downloads do sistema, escrita por
  extenso na confirmação. Um seletor nativo custaria um crate a mais na árvore,
  e a lista de consequências abaixo diz que nenhuma dependência nova entra por
  causa disto.

  **Isto foi revertido em 2026-08-18**, pela primeira pessoa que usou. O
  seletor existe, custou três crates contados, e a seção «O que a primeira
  pessoa a anexar um arquivo ensinou» conta o resto. Escolher continua sendo
  arrastar **também**.

As quatro coisas de «o que fica sem saída» continuam sem saída, e nenhuma delas
foi tocada.

## Contexto

Item 6 da lista do parceiro humano depois do teste em rede local, no mesmo
formato das duas queixas que originaram o ADR 0026: não dá para mandar imagem,
nem áudio, nem arquivo. Depois que a limitação de taxa fechou (ADR 0025) e que a
escada de alcance subiu dois degraus (ADR 0022), é a maior lacuna funcional que
sobrou — e é a única que uma pessoa nota nos primeiros cinco minutos de uso.

Havia uma posição escrita sobre isto, e ela era o contrário desta.
`specs/02-protocolo.md`, em "Decisões em aberto": *"**[EM ABERTO]** Limite de
tamanho de mensagem e política de anexos (v1 tem anexos? provavelmente não)."* E
a decisão **D14** do plano (`docs/plano-m0-m1.md`, §4.2) responde: *"Sem anexos
em v1. Teto de corpo em 4 KiB, fixado já em M0 porque `08` exige limite de
tamanho antes de alocar"*.

A metade do teto foi construída — `MAX_BODY_LEN` está em
`crates/seele-proto/src/control.rs` e vale 4 KiB. A metade do "sem anexos" é o
que este ADR desfaz, e desfaz de propósito: 4 KiB de corpo continua certo, e
"nenhum arquivo" deixou de ser.

O que existe hoje, e que condiciona todo o desenho abaixo:

- **Um único fluxo QUIC bidirecional por conexão.** O cliente o abre
  (`crates/seele-core/src/client.rs:248`), o Dogma o aceita
  (`crates/seele-server/src/session.rs:163`), e ele carrega **tudo** o que não é
  voz: aperto de mão, presença, comandos, texto e histórico. O comentário de
  cabeçalho do `control.rs` diz que o texto iria em fluxos efêmeros próprios
  "para que buscar cinco mil mensagens de histórico não atrase um evento de
  presença". Isso está escrito e não está construído: não há um `open_uni` em
  lugar nenhum do repositório.
- **Enquadramento** de quatro bytes de tamanho em big-endian mais o quadro,
  `postcard`, com teto de `MAX_FRAME_LEN` = 16 KiB conferido **antes** de
  alocar (`crates/seele-core/src/frame.rs`).
- **Voz em datagrama.** `MAX_DATAGRAM_LEN` é 1286 e **não tem nada a ver com
  isto**: o maior datagrama de voz que este build produz tem 272 bytes, medido
  em `codec.rs` e travado em teste (pendência 15). Anexo não é voz e não passa
  por ali.
- **Um banco só.** `seele.db`, SQLite pelo `rusqlite` com `bundled`, caminho
  vindo de `SEELE_DB` e com padrão no diretório corrente
  (`crates/seele-server/src/main.rs`). O arquivo TOML que `specs/04-servidor-seele.md`
  descreve **não existe**: `DogmaConfig` é uma struct montada no `main.rs`, e o
  doc dela diz por quê — "M2 toma os mesmos campos como struct e deixa a análise
  para M3". A única configuração que sobrevive a reinício é a tabela
  `configuracao`, que a migração 2 criou para o ADR 0021.
- **Uma poda de histórico que ninguém ligou.** `Messages::prune(retention_days)`
  existe em `crates/seele-server/src/persistence/messages.rs` e **só é chamada pelo
  próprio teste**. `specs/04` prevê `retencao_dias = 0` — ilimitado — no arquivo
  que não existe. Guardar isto na cabeça importa para a alternativa 2.
- **Doze permissões enumeradas**, sem sistema de expressão, com quatro papéis
  semeados no esquema (`crates/seele-server/src/persistence/schema.rs`).

## Decisão

**O Dogma guarda os anexos, com teto total fixo, e o mais velho sai quando
enche.** O padrão é 1 GiB; quem hospeda escolhe o número. Ao encher, o anexo
mais antigo é descartado, e a mensagem que o carregava passa a dizer que o
arquivo expirou — o texto sobrevive ao anexo.

O motivo da escolha é um só, e é o que separa esta opção das outras três: **um
Dogma doméstico roda no notebook de alguém, e o pior caso de disco tem que ser
conhecido no dia um.** Não "provavelmente uns dois giga", não "depende de
quanto o pessoal usar": um número que a pessoa escolheu antes de a primeira foto
chegar, e que o produto não ultrapassa por construção.

### O caminho do arquivo

**Um fluxo QUIC unidirecional por transferência, e nunca o fluxo de controle.**

O fluxo de controle é ordenado, e um fluxo ordenado bloqueia a si mesmo: 20 MB
escritos nele param toda presença, todo texto e todo `Pong` de todo mundo atrás
deles até o último byte passar. É o único fluxo que não pode enfileirar.

Quem envia abre um `open_uni`, escreve um cabeçalho — Linha, `client_message_id`,
tamanho declarado, tipo declarado, hash do conteúdo — e em seguida os bytes. A
**resposta volta pelo fluxo de controle**, como quadro enumerado, porque
`specs/02-protocolo.md` manda que toda razão seja um enum e é no controle que as
razões já moram. Para baixar, o cliente pede no controle e o Dogma abre um
unidirecional de volta.

**Prioridade.** `quinn::SendStream::set_priority` existe, e o controle fica
acima de toda transferência. Vale dizer o que isso **não** faz: prioridade
ordena os nossos fluxos dentro de **uma** conexão. Duas pessoas subindo arquivo
de duas conexões diferentes não se ordenam entre si, e não há resposta boa para
isso num enlace doméstico — ver "O que fica sem saída".

**Pedaços.** Nenhum enquadramento de pedaço na rede: o fluxo QUIC já é a
fronteira, e inventar um segundo enquadramento por cima de um transporte que já
entrega em ordem é código sem comprador. O pedaço existe **no disco**: quem
recebe grava em blocos de tamanho fixo e nunca segura o arquivo inteiro em
memória — `specs/04-servidor-seele.md` dimensiona o Dogma em 1 vCPU e 512 MB, e
um `Vec` de 20 MB por transferência simultânea acaba com isso.

**Não há retomada.** Uma transferência que cai recomeça do zero. O QUIC não
oferece continuação de fluxo entre conexões, e construir deslocamento e retomada
é um segundo desenho inteiro. Nos tamanhos que o teto por arquivo permite, é
irritação e não tragédia — mas é irritação real num enlace ruim, e fica escrito
em vez de descoberto.

### Quem paga a espera

**Quem manda vê a mensagem na hora, num estado de envio, com barra por bytes.**
É a mesma forma que o ADR 0026 já fixou para o atualizador: quando o total é
conhecido, barra com porcentagem; quando não é, travessão com o motivo. Aqui o
total é sempre conhecido — quem escolheu o arquivo sabe o tamanho dele —, então
é sempre barra, e nunca uma barra fingindo medir o que ninguém mediu.

**A mensagem só é publicada na Linha depois de os bytes chegarem inteiros.** O
custo é que, enquanto sobe, só quem enviou a vê. O ganho é que ninguém mais
precisa de um segundo estado: sem isso, "o anexo ainda não chegou" e "o anexo
expirou" seriam duas ausências parecidas na mesma tela, e o ADR 0022 já pagou
uma vez por duas coisas diferentes com a mesma aparência.

**Se cair no meio**, o Dogma descarta o parcial e não publica nada. Não fica meia
mensagem, nem mensagem apontando para arquivo que não existe. Do lado de quem
enviou, a mensagem local fica em estado de falha, com uma tentativa que recomeça
do zero.

A repetição é segura, e é de graça: `client_message_id` já existe e já torna o
envio idempotente — é a lacuna G9, com índice único parcial
`messages_idempotency` no esquema. A transferência é chaveada **pelo mesmo**
`client_message_id`, então uma retentativa depois de uma queda que na verdade
tinha dado certo do lado do Dogma é reconhecida antes de 20 MB subirem de novo.

### O teto, de verdade

**Quem configura:** quem hospeda, por subcomando do `seeled`, ao lado dos dois
que já existem (`seeled convite`, `seeled senha`). **Onde mora:** na tabela
`configuracao`, e não no arquivo TOML de `specs/04`. O critério é o que a própria
migração 2 escreveu ao criar a tabela — "configuração do Dogma que não cabe num
arquivo, porque muda em tempo de execução e precisa sobreviver a reinício". O
teto é exatamente isso: mexer nele com o Dogma no ar é o caso normal, e um
arquivo que ainda não existe não vai nascer por causa de um número.

**Onde os bytes moram:** num diretório `anexos/` ao lado do `seele.db`, e
**não dentro dele** — ver as alternativas.

**No instante em que enche:** a conferência acontece **antes de aceitar**, contra
o tamanho declarado, e o descarte também. Se o que já está guardado mais o que
foi declarado passa do teto, o Dogma descarta os mais antigos até caber e só
então lê o primeiro byte. Descartar depois deixaria o disco acima do teto por
alguns segundos, e um teto que é ultrapassado por trinta segundos não é um teto —
é a propriedade inteira desta decisão que se perde. Tamanho declarado que mente
morre no fim: o fluxo é cortado no tamanho declarado e o hash não fecha.

**Teto por arquivo, derivado e não configurado.** Um arquivo de 900 MiB num
Dogma de 1 GiB esvaziaria o histórico de todo mundo num ato só. Então há um teto
por arquivo, e ele é uma **fração do total**, calculada, para que os dois números
não possam ser configurados num par absurdo. Um arquivo maior que esse teto é
recusado com razão enumerada, e não aceito para ser jogado fora depois.

**Como a mensagem antiga fica.** A linha em `messages` não é tocada — nem
`body`, nem `deleted_at`, nem nada. O anexo mora numa tabela nova, e expirar
**apaga os bytes e mantém a linha**, marcando quando expirou. A linha sobrevive
para que a mensagem consiga dizer «este arquivo expirou», com o nome original e
o tamanho: uma mensagem cuja linha de anexo tivesse sido apagada renderizaria
como uma mensagem com nada, e ninguém saberia que houve um arquivo ali.

A consequência no banco está escrita e é aceita: **a tabela de anexos nunca
perde linhas, só bytes.** Ela cresce para sempre. Uma linha são algumas dezenas
de bytes e um Dogma de amigos não vai notar em anos — mas é crescimento sem teto
dentro de um ADR cuja tese é ter teto, e esconder isso seria contradizer o
próprio texto. É a mesma regra de `docs/pendencias.md`: o que fecha fica na
lista, marcado, porque some-lo apaga a história junto.

O estado de expirado é **enumerado**, e não uma frase: a casca decide como
apresentar cada variante, como `specs/02-protocolo.md` exige de toda razão.

### Identidade do arquivo

**Endereçamento por conteúdo, por SHA-256.** Duas pessoas mandando a mesma foto
guardam **uma** cópia. O `sha2` já está na árvore do `seele-proto`, então nada
novo entra por causa disto.

Três problemas vêm junto, e os três têm resposta:

**Apagar a cópia de uma pessoa não pode apagar a da outra.** O arquivo é contado
por quantas linhas vivas apontam para aquele hash. `Permission::RemoveMessage`
já existe e já apaga mensagem de outro; apagar expira **a linha**, e os bytes só
vão embora quando a última linha que os referenciava se for. É contagem, e é
testável.

**Deduplicar cria um oráculo, e este desenho recusa o oráculo.** Se o Dogma
respondesse "já tenho esse conteúdo" antes de receber, quem enviasse aprenderia,
pelo tempo, que **alguém já mandou aquele arquivo exato** — inclusive numa Linha
que a pessoa não pode ler. Então o Dogma **recebe os bytes de qualquer jeito** e
descarta depois se já os tiver. O que isso custa é a subida de quem envia; o que
a deduplicação existe para poupar é o **disco do anfitrião**, que é o recurso sob
teto rígido. O recurso que importa continua poupado, e o oráculo não chega a
existir.

**O nome é dado, não é endereço.** O nome original é coluna, nunca caminho. Duas
pessoas mandando o mesmo conteúdo com nomes diferentes têm duas linhas e um
arquivo.

### O que o SEELE promete sobre o conteúdo

Nada de novo, e é justamente por isso que precisa ser dito em voz alta.

`specs/08-seguranca.md` já traz, na tabela do modelo de ameaça, *"Vazamento de
histórico por acesso ao disco do servidor — **Fora de escopo em v1**, documentar
claramente"*, e mais abaixo, *"em v1, o operador do servidor pode, em teoria,
capturar mídia. Isso é aceitável para o modelo de uso (você hospeda para seu
próprio grupo), mas precisa estar escrito."* Um anexo guardado no Dogma é
legível por quem tem o disco do Dogma. Isso é aceitável, pela mesma razão que o
histórico de texto já é — e **tem que ser dito**, não decidido no susto.

Mas há uma diferença de grau que merece a frase própria: o histórico de texto
está dentro de um SQLite e exige saber perguntar. Um diretório `anexos/` é
navegável por qualquer gerenciador de arquivos, e uma foto se lê de relance. O
disco que já guardava a conversa passa a guardar algo que qualquer um folheia
sem saber SQL. É diferença de esforço, não de direito — e é a diferença que faz
alguém se surpreender.

**Guardado em claro, e documentado.** Cifra em repouso foi considerada e
recusada: a chave teria de estar viva no mesmo disco, porque o Dogma precisa
servir o arquivo a qualquer momento a quem tem permissão. Isso protege contra
notebook roubado e contra nada mais — e contra notebook roubado a cifra de disco
inteiro do sistema protege melhor, é o que a pessoa já tem, e ela pode ligar hoje.

O que **não** muda: em trânsito continua TLS 1.3 dentro do QUIC, sem modo claro e
sem interruptor; TOFU e fixação de impressão digital (ADRs 0003 e 0004) valem
igual. O arquivo não é legível na rede. É legível no disco de quem hospeda.

A consequência de produto, dita na frase mais curta que ela cabe: **uma foto
mandada num Dogma é uma foto no notebook de alguém, e quem hospeda pode vê-la.**
Quem entra num Dogma tem de saber disso antes de mandar a primeira.

### Abuso

O ADR 0025 tem dois baldes de fichas. Um serve, o outro não, e a diferença é
exata.

**Serve, e sem mudança nenhuma:** o balde da portaria, por endereço de origem,
antes de autenticar. Anexo não passa por ali e não muda nada ali.

**Não serve:** o balde por conexão, depois de autenticar. Ele conta **quadros**,
não bytes — `QUADROS_DE_RAJADA` = 60, `QUADROS_POR_SEGUNDO` = 20. Um anexo de
20 MB é um quadro de controle e um fluxo. Sessenta quadros de rajada são cegos
para gigabytes. Esse balde não protege disto em grau nenhum, e dizer que
"já temos limitação de taxa" seria falso.

**O que protege é o teto.** Por construção, o disco não passa dele. Foi para
isso que o teto foi escolhido, e é o único mecanismo aqui que é um **limite** e
não um retardo.

**O que o teto não protege é a justiça**, e isso é dito na seção própria abaixo.

**Um terceiro balde, contando bytes por piloto**, entra junto — no mesmo
`crates/seele-server/src/taxa.rs`, no mesmo mecanismo, com o tempo entrando por
parâmetro como o 0025 já faz. Ele não é um limite: é um retardo. Ele transforma
"esvaziar o Dogma inteiro em dois minutos" em "esvaziar ao longo de horas", que
é tempo suficiente para quem hospeda notar e usar `Kick` ou `Ban`, que já
existem. Chamá-lo de proteção seria exagero; ele compra tempo, e tempo é o que
faltava.

**A permissão é nova: `Permission::AttachFile`**, acrescentada **no fim** da
enumeração, pela razão que o próprio `control.rs` já escreve sobre as variantes
de sala — um build uma versão de protocolo mais velho recusa o quadro em vez de
lê-lo como outra coisa que entende.

Não é dobrada no `WriteLine`. "Pode escrever" e "pode pôr um gigabyte no meu
notebook" são perguntas diferentes, e o sentido de hospedar para os seus amigos é
poder responder a segunda separadamente. Papéis semeados: Commander, Operator e
Pilot ganham; **Observer nega explicitamente**, e não por ausência — o próprio
esquema já explica por quê, na linha do Observer: negar de propósito faz com que
dar Observer a alguém que também é Pilot **silencie**, em vez de não fazer nada.

Isso implica migração de esquema, e não só variante nova: os papéis estão
semeados como JSON no banco, e um Dogma que já existe precisa da linha atualizada
na próxima subida.

### O que se recusa a receber

Um Dogma doméstico **não varre vírus**. Não há motor aqui, não há base de
assinaturas, e não há caminho de atualização para uma. Dizer o contrário seria
pior do que não dizer nada.

As `NOTAS-DE-RELEASE.md` deste projeto já fazem a separação, e fazem em três
degraus, na seção "Como conferir que o arquivo é o que diz ser": *"Duas
perguntas diferentes, e cada uma tem a sua resposta."* — *"O arquivo chegou
inteiro?"* responde a soma; *"e como sei que este arquivo veio daquele código?"*
responde o atestado de procedência; e o terceiro degrau é o que fecha a seção:
*"O que o atestado **não** faz é dizer que o software é bom. Ele diz de onde
veio. As duas coisas são frequentemente confundidas."*

Anexo herda os três degraus e só alcança o primeiro. Do segundo não há sequer
mecanismo — não há workflow que tenha produzido a foto de alguém.

**O que ele faz:**

- Confere o tamanho contra o declarado e **o conteúdo contra o hash declarado**,
  antes de publicar coisa nenhuma. É a pergunta "chegou inteiro", e ela tem
  resposta aqui.
- Recusa, com razão enumerada, o que não fecha — e não publica mensagem.
- Guarda **sob o hash do conteúdo, nunca sob o nome que quem enviou escolheu.**
  Nenhum caractere escolhido por outra pessoa toca o sistema de arquivos: nada de
  `../`, nada de `CON`, nada de byte nulo, nada de duas maiúsculas colidindo no
  macOS ou no Windows. O nome é dado numa coluna.
- Registra o tipo declarado e o trata como **alegação**. Só uma lista curta de
  tipos de imagem é desenhada embutida na conversa, e só quando os bytes
  concordam com a alegação. Todo o resto é um arquivo com nome e tamanho, sem
  prévia. Isso não é sobre confiar no arquivo: é sobre para qual decodificador
  os bytes vão.

**O que ele não faz:** varrer, garantir que o tipo é o que diz, nem afirmar
coisa alguma sobre o arquivo ser bom. A primeira pergunta tem resposta. A segunda
não tem, e nenhuma frase deste produto vai fingir que tem.

### Executável é o caso difícil

Guardar e reentregar `.exe` num chat é literalmente como malware anda entre
amigos. Não é o SEELE que vira antivírus, e fingir que a questão não existe seria
desonesto. A política, inteira:

**Guarda, nunca desenha, nunca abre.** Não existe botão de abrir num cliente do
SEELE. Salvar é um ato de quem recebeu, num lugar que a pessoa escolheu. É o
único ponto deste desenho em que dá para ser estrito, e ele é estrito.

**Não renomeia, não corta extensão.** Renomear `.exe` para parecer inofensivo faz
o arquivo mentir, e mentir é a última coisa que ajuda aqui.

**Ao gravar, marca com a marca do próprio sistema.** `com.apple.quarantine` no
macOS, o fluxo `Zone.Identifier` no Windows. É o que faz o Gatekeeper e o
SmartScreen — os mesmos dois de que trata o ADR 0026 — pararem o arquivo na
frente de quem for abri-lo. Não é antivírus, é a guarda que o sistema já tem e
que só funciona se quem grava a acionar. É o mesmo mecanismo do navegador, e é a
resposta concreta que este produto **pode** dar.

**O Dogma não recusa por extensão.** Uma lista de extensões proibidas é
contornada com um `rename`, quebra usos legítimos — mandar a um amigo um build
deste próprio projeto é um `.exe` — e, pior que as duas coisas, faz o que passou
parecer conferido. Uma lista que não para ninguém e tranquiliza todo mundo é pior
que lista nenhuma.

**O que sobra dito, e não resolvido:** o SEELE vai reentregar malware entre
amigos, se um amigo mandar. Faz o mesmo que entregar um pendrive na mão, com a
mesma quantidade de conferência, e a guarda é a do sistema operacional e a da
pessoa. Está escrito aqui para que ninguém precise descobrir sozinho.

## Alternativas consideradas

1. **Não guardar nada, entrega direta entre as pessoas.** Zero disco, o que é
   sedutor num produto cuja tese é o notebook de alguém. Recusada por duas
   razões, e a segunda é fatal: só entrega com as duas pontas **online ao mesmo
   tempo**, e mandar foto para quem está dormindo é o uso mais comum que existe;
   e "direto" exigiria cada cliente alcançável pelo outro, quando o ADR 0022
   gastou uma escada inteira para estabelecer que um cliente atrás de roteador
   doméstico **não é** alcançável. Direto de verdade dependeria do degrau 4, que
   não existe e que custa a conversa sobre metadado que aquele ADR guarda.

2. **Teto por arquivo mais prazo de validade.** O disco cresce e encolhe
   sozinho, sem ninguém escolher número nenhum — é a opção mais elegante das
   quatro. Recusada porque **o pior caso não é previsível**: muita gente ativa
   numa semana enche qualquer disco antes de qualquer prazo vencer, e o produto
   não teria como dizer, no dia um, quanto vai ocupar. Num servidor alugado isso
   é um susto; num notebook em que a pessoa também trabalha, é como um app de
   conversa vira a coisa que encheu o disco dela. Há uma evidência caseira a
   favor da recusa: a poda por prazo **já existe** para o texto —
   `Messages::prune(retention_days)` — e nunca foi ligada, porque prazo nenhum
   respondia à pergunta que alguém de fato tem, que é quanto isto vai ocupar.
   Construir a segunda cópia do mesmo mecanismo não ligado seria escrever duas
   vezes a mesma resposta que não serve.

3. **Só imagem, e pequena.** Menor risco de todos, e o que menos atende ao que
   foi pedido. Recusada porque não responde a «mandar arquivo», que é metade do
   pedido — e porque construiria o mecanismo **inteiro** de qualquer jeito: o
   teto, o armazenamento, o fluxo próprio, a permissão, a política de conteúdo.
   Todo o custo, e a recusa justamente do caso que motivou o trabalho.

4. **Guardar os bytes dentro do `seele.db`.** Um arquivo só para fazer cópia de
   segurança, uma coisa só para manter consistente, e o teto seria um
   `PRAGMA page_count`. Recusada pelo `VACUUM`: apagar um blob libera páginas
   **dentro** do arquivo e não encolhe o arquivo, então o pior caso em disco
   passaria a ser a maré alta histórica e não o teto — que é exatamente a única
   propriedade pela qual esta decisão foi tomada. Some-se a isso que o Dogma é
   dimensionado para 1 vCPU e 512 MB, e que blob grande em SQLite só evita a
   alocação inteira pelo caminho de I/O incremental, que é mais uma coisa a
   acertar para ficar pior no fim.

5. **Pedaços como quadros de controle no fluxo que já existe.** Zero fluxo novo.
   Recusada: 20 MB em quadros de 16 KiB são 1280 quadros, que a
   `QUADROS_POR_SEGUNDO` = 20 do ADR 0025 leva mais de um minuto para deixar
   passar — e que, no caminho, ficariam intercalados com o texto de todo mundo
   no mesmo fluxo ordenado. É o bloqueio de cabeça de fila que este desenho
   existe para evitar, e ele cairia em cima da **pendência 1**, que é sobre
   corpos grandes nesse mesmo fluxo e continua sem diagnóstico.

## Consequências

- **A maior lacuna funcional fecha**, e o pior caso de disco passa a ser um
  número que quem hospeda escolheu antes da primeira foto.
- **Duas coisas para copiar em vez de uma**, e elas podem divergir: um arquivo
  órfão em `anexos/`, ou uma linha apontando para um arquivo que sumiu. A regra
  é que **a linha é a verdade** — arquivo faltando se lê exatamente como
  expirado, que é um estado que o desenho já tem —, e órfão é varrido na subida.
- **A tabela de anexos cresce para sempre**, em linhas, porque expirar apaga
  bytes e não linhas. É pequeno, é lento, e está escrito.
- **Uma migração que reescreve os papéis semeados**, porque a permissão nova
  precisa entrar em Dogmas que já existem: as permissões são JSON dentro da
  coluna `permissions` de `roles`, e a resolução em `melchior.rs` percorre a
  enumeração inteira, então a décima terceira variante é mecânica no código e
  não é mecânica no banco.
- **Nenhuma dependência nova**, e por isso nenhuma exceção nova no `deny.toml`.
  O `sha2` já está na árvore do `seele-proto` — é o que calcula a impressão
  digital do certificado — e o `quinn` já traz fluxo unidirecional e prioridade.
  O `blake3` seria mais rápido e não entra: o ganho não paga um crate a mais
  numa árvore que o ADR 0026 acabou de contar crate a crate.

  **Esta consequência caiu em 2026-08-18**, e só ela: as três dependências que
  entraram são do seletor de arquivos, e nenhuma delas toca o caminho dos bytes
  que o resto desta lista descreve. O `blake3` continua fora pelo mesmo motivo
  de sempre, que não era o teto de dependências e sim o ganho. A seção «O que a
  primeira pessoa a anexar um arquivo ensinou» conta o que foi medido e por que
  a decisão virou.

  **A prévia, no mesmo dia, não custou nenhuma.** Quem decodifica é o motor do
  WebView, que já está no processo; o base64 são vinte linhas em vez de um
  crate; e farejar quatro assinaturas de doze bytes não é trabalho de
  biblioteca. A conta está na seção «O que os primeiros bytes decidem».
- **A versão de protocolo sobe.** Variante nova em `ClientMessage`, em
  `ServerMessage`, em `Permission` e em `AlertReason`, todas no fim da
  enumeração. Um cliente uma versão atrás recusa o quadro em vez de o ler
  errado, e perde a conexão ao encostar no recurso — é a mesma troca que o ADR
  0025 já aceitou por `AlertReason::RateLimited`.
- **A voz sofre durante uma subida**, num enlace doméstico. Prioridade de fluxo
  não conserta isso, porque voz não é fluxo: é datagrama, e concorre pelo mesmo
  gargalo de saída. O ADR 0009 tem orçamento de latência medido, e este é o
  primeiro recurso do produto capaz de estourá-lo de propósito. A única alavanca
  real é limitar a taxa de saída de cada transferência, e **este ADR não escolhe
  o número**, porque não há número honesto sem medir — a medição tem a forma do
  `examples/cadencia`, e é trabalho próprio.
- **A pendência 1 não é consertada por isto, e não deve ser lida como se
  fosse.** Ela é sobre rajada de corpos grandes no fluxo de controle, e continua
  sem diagnóstico. Este desenho apenas não põe volume ali.

## O que fica sem saída

Quatro coisas, e nenhuma delas tem resposta boa. Estão aqui porque um ADR que só
louva a opção escolhida não serve para nada daqui a um ano.

**Justiça sob teto global.** Uma pessoa com a permissão pode, em algumas
subidas, expulsar do disco todos os anexos que todo mundo mandou. O disco
continua limitado; o histórico não continua. O balde de bytes atrasa e não
impede. Cota por pessoa foi pensada e não fecha: N cotas só são um limite se N
for limitado, e um Dogma sem senha e sem convite é **aberto** por padrão
(ADR 0021) — o `seeled` avisa em voz alta ao subir assim fora do loopback, mas
avisar não é limitar. **Só o teto global é um limite.** A escolha aqui é entre
uma garantia de disco com histórico frágil e uma garantia de histórico sem
garantia de disco, e o dono escolheu a primeira sabendo disto.

**Retomada.** Transferência que cai recomeça do zero.

**Concorrência entre conexões.** Duas pessoas subindo ao mesmo tempo não se
ordenam; a prioridade de fluxo só vale dentro de uma conexão.

**Quem hospeda lê tudo.** Não há versão desta decisão em que não leia, aquém de
E2EE — que é pós-v1 por `specs/08-seguranca.md`, e que quebraria de uma vez a
deduplicação por conteúdo e a contabilidade do teto, porque o Dogma deixaria de
conseguir dizer que dois anexos são o mesmo.

## Custo de reverter

**Baixo antes do primeiro anexo guardado:** uma migração, uma permissão, um
fluxo, um subcomando.

**Depois, tem uma propriedade que vale escrever:** desligar anexos num Dogma que
já os tem não quebra histórico nenhum, porque o estado de saída já existe. Todo
anexo expira de uma vez, e cada mensagem passa a dizer «este arquivo expirou» —
que é a frase que ela já saberia dizer. O estado de expirado é também a porta de
saída, e isso não foi acidente.

## O que a primeira pessoa a anexar um arquivo ensinou

Escrito em 2026-08-18, depois do relato de campo: a 0.6.0 instalada num macOS
de verdade, e duas queixas no mesmo fôlego — *arrastei um arquivo para a
conversa e não aconteceu nada*, e *cliquei no botão ARQUIVO e não abriu nada*.

Tudo o que este ADR desenhou estava construído: `enviar_anexo`,
`descrever_arquivo`, `salvar_anexo`, a barra por bytes, as dez recusas com dez
frases. Nada disso alcança ninguém com as duas portas de entrada fechadas, e
elas estavam fechadas por dois motivos que não têm nada em comum. Vale separar,
porque a mesma frase — «não aconteceu nada» — cobria as duas.

### Arrastar não falhou no desenho, falhou na permissão

`listen("tauri://drag-drop", …)` não é chamada de JavaScript. É chamada de IPC
ao plugin `event`, e na Tauri v2 **todo** comando de plugin passa pelo sistema
de capacidades antes de alcançar código nenhum
(`tauri-2.11.5/src/webview/mod.rs`: se o comando é de plugin e a ACL resolvida é
`None`, o pedido é recusado ali e não vai adiante). Este app não tinha
`capabilities/` — o `gen/schemas/capabilities.json` gerado ao lado dele era
`{}` —, então a chamada voltava com «event.listen not allowed», a promessa
rejeitava para ninguém, e o ouvinte ficava escrito no arquivo sem nunca receber
nada.

**Todo** `listen(...)` do frontend estava morto pelo mesmo motivo: os dois laços
de snapshot e o do atualizador junto. Só o arrastar foi notado, e a razão é o
que torna este defeito instrutivo: os dois laços de snapshot têm um
`setInterval` de 500 ms ao lado, que redesenha a tela de qualquer jeito. Uma
rede de segurança escondeu a queda por meses; a única função sem rede foi a
única a aparecer.

**A suspeita natural estava errada, e fica escrita porque custa uma tarde.** A
leitura razoável era que os eventos de arrastar da v2 são *da webview* e que um
`listen()` global nunca os receberia — o nome do evento mudou de
`tauri://file-drop` para `tauri://drag-drop` entre as versões, o que sugere que
essa área mexeu. Na 2.11.5 não é assim: o `drag-drop` é emitido com filtro por
webview, mas o casamento é feito por `match_any_or_filter`
(`src/event/listener.rs`), e a primeira coisa que essa função faz é aceitar todo
ouvinte cujo alvo é `EventTarget::Any` — que é exatamente o alvo que o `listen()`
do JavaScript registra por omissão. O filtro só decide sobre ouvintes que
pediram um alvo específico. E `dragDropEnabled` não precisava ser declarado:
`tauri-utils` o inicializa em `true`.

### O botão falhou no desenho, e o desenho era deste ADR

O botão ARQUIVO não abria nada porque **não havia o que abrir**: a lista acima
decidiu que escolher um arquivo era arrastá-lo, e o botão existia para ensinar
isso, dizendo a instrução em voz alta. Ele clicou esperando um seletor.

Arrastar-e-soltar não se descobre sozinho. Um botão escrito ARQUIVO promete um
seletor a qualquer pessoa que já usou um computador, e um botão que promete uma
coisa e faz outra é pior do que botão nenhum — porque ele consome a tentativa.

E «nenhuma dependência nova» era **consequência escolhida por quem escreveu este
ADR**, não restrição dada pelo dono. Uma consequência que custa a única porta de
entrada que uma pessoa encontra sozinha é uma consequência cara demais.

**O custo, medido como o ADR 0026 mediu.** `tauri-plugin-dialog 2.7.2` entra, e
com ele entram exatamente três crates: ele, `tauri-plugin-fs 2.5.1` — de onde vem
o tipo `FilePath` que o seletor devolve — e `rfd 0.16.0`. Três, e os **mesmos**
três em cada um dos três alvos, contados com `cargo tree --no-dedupe` em
`aarch64-apple-darwin`, `x86_64-unknown-linux-gnu` e `x86_64-pc-windows-msvc`: o
`rfd` fala com a API nativa de cada sistema, e as três já estavam na árvore por
causa do próprio Tauri. Licenças: `MIT` no `rfd`, `Apache-2.0 OR MIT` nos dois
plugins — nenhuma exceção nova no `deny.toml`, e `cargo deny check` continua com
`bans ok, licenses ok, sources ok`.

**A saída sem dependência foi procurada e não existe.** `<input type="file">`
numa webview Tauri devolve um `File` do navegador e **não** um caminho no disco,
que é a única coisa que `descrever_arquivo` e `enviar_anexo` sabem usar — ler os
bytes pelo JavaScript para reenviá-los pelo IPC seria carregar o arquivo inteiro
na memória da janela, que é justamente o que a seção «Pedaços» acima proíbe do
outro lado. Chamar o diálogo do sistema por fora — `osascript` no macOS,
PowerShell no Windows, `zenity` no Linux — não custa crate e custa três caminhos
de código, um binário externo por plataforma, e um Linux sem `zenity` instalado
fica sem seletor. Três crates auditados são mais baratos que isso.

**Quem abre o diálogo é a casca, e não a página.** O seletor é um comando desta
casca, `escolher_arquivo`, com o título escrito no `main.rs` e uma coisa só que
ele faz. `capabilities/janela.json` não dá à página permissão de `dialog`
nenhuma — nem de `fs`, que veio junto na árvore —, e um teste confere isso pela
**palavra** da recusa, porque um plugin ausente também falha e um guarda que só
pedisse falha passaria vazio. Somar um plugin não pode alargar calado o que a
página pode fazer.

Escolher continua sendo arrastar **também**: quem arrasta espera que arrastar
funcione, e agora funciona.

### A terceira coisa, que ninguém relatou e que morde de novo

Ao clicar no botão, ele não viu **nem o anúncio**. Não era o ouvinte: era o
`anunciar(...)`. A `.anuncio` é uma caixa de um pixel com `clip-path:
inset(50%)`, que existe para leitor de tela — quem enxerga não vê nada. Uma
falha contada só ali e coisa nenhuma são o mesmo acontecimento para quem está
olhando para a janela.

O caminho de anexos tinha isso em toda recusa de leitura, e continuaria tendo
depois do seletor. Agora a recusa é escrita na caixa do anexo, que já é
`aria-live` e diz as duas coisas de uma vez. Um guarda lê as folhas de estilo,
descobre **quais** regiões estão recortadas fora da tela, e reprova se tudo o
que a recusa escreve cair dentro delas — a propriedade, e não o nome do
elemento.

### O que os guardas não pegavam, e agora pegam

`tests/frontend.rs` sabia provar que o ouvinte estava **escrito** no arquivo.
Nenhum teste deste repositório sabia perguntar se ele era **registrado**, e a
distância entre as duas coisas era o defeito inteiro.

`apps/seele-app/tests/permissoes.rs` é a metade que faltava: constrói o app de
verdade — este `tauri.conf.json`, esta ACL gerada — com o runtime falso, põe uma
webview na frente dele e faz a mesma chamada de IPC que a página faz. Sem o
arquivo de capacidade, ele reprova com a frase exata que o macOS devolveu.

Fica dito o que **não** dá para cobrar sem uma janela aberta, para ninguém
escrever um teste que passa sempre no lugar: que o sistema operacional emita o
evento de arrastar, que o diálogo apareça na tela, e que a pessoa consiga
apertar o botão dele. Os três dependem de um servidor gráfico e de um gesto
humano. O que dá para cobrar é tudo o que acontece **antes** deles, e é isso que
está coberto.

## O que os primeiros bytes decidem

Escrito em 2026-08-18, depois do mesmo relato de campo: *preview de
imagem/documento anexo*. Todo anexo aparecia como nome e tamanho, e nada mais —
o lado seguro da regra que este ADR escreveu, e não o lado completo. O que
segue é o outro lado, construído.

O que **não** mudou é o começo desta seção: a regra é a mesma. Só uma lista
curta de tipos de imagem é desenhada embutida, e só quando os bytes concordam
com a alegação. Nenhuma linha abaixo afrouxa isso; elas todas explicam como ela
foi cumprida.

### O nome é texto que a outra pessoa escolheu

É a frase que carrega tudo. Um anexo chega com um nome e um tipo declarado, e
os dois foram escritos por quem mandou. Desenhar uma imagem porque o nome
termina em `.png` é entregar a escolha do decodificador à ortografia de um
estranho — e escolher decodificador é exatamente a única coisa que este ADR já
tinha dito que a alegação não pode fazer.

Então a decisão sai dos **primeiros doze bytes**, em
`crates/seele-core/src/preview.rs`, e o tipo de mídia que a janela recebe é
escrito a partir do que foi **achado**, nunca do que foi alegado. O veredito não
carrega uma `String` vinda da rede: carrega uma variante de um enum fechado, e o
`data:` inteiro é montado do lado do Rust, tipo de mídia incluído. A página não
junta bytes com tipo. Uma página que juntasse poderia juntar com a alegação, e é
por isso que ela não junta.

**As duas metades têm de concordar.** Farejar sozinho faria o nome virar
enfeite; a alegação sozinha seria confiar em quem mandou. Só desenha quando as
duas dizem a mesma coisa — o que significa, dito por extenso, que bytes de PNG
perfeitos chegando como `application/octet-stream` **não** são desenhados. É a
palavra «concordam» levada a sério nas duas direções.

### Quatro formatos, e o motivo de cada um

Cada formato que se desenha é um decodificador que vai ler bytes de terceiro. O
motor do WebView faz esse trabalho, mas escolher entregar um formato a ele é uma
decisão, não um acaso. Então cada um vem com a razão:

- **PNG** e **JPEG** — o que uma captura de tela e uma câmera produzem. Sem os
  dois o recurso não existe.
- **GIF** — o que as pessoas de fato colam numa conversa.
- **WebP** — o que o «salvar imagem» de um navegador escreve hoje. Deixá-lo de
  fora faria «salvei da web e mandei» ser a única coisa que não desenha.

E o que ficou de fora, com o motivo, porque uma lista que ninguém defendeu
cresce:

- **SVG** é marcação, não imagem: vai para o mesmo interpretador da página, pode
  carregar script, e não tem assinatura para farejar porque é texto.
- **PDF** é um documento com um interpretador atrás. Todo o argumento deste ADR
  sobre para qual decodificador os bytes vão pesa mais aqui do que em qualquer
  outro lugar.
- **HEIC** e **AVIF** são farejados por marca dentro de uma caixa ISO-BMFF que o
  `mp4` divide com eles, então separar foto de contêiner de vídeo é confiar numa
  cadeia de quatro bytes; e o suporte deles difere entre os três alvos, o que
  faria o mesmo arquivo desenhar no macOS e não no Linux. Um produto, três
  sistemas.
- **BMP**, **ICO** e **TIFF** têm assinatura de dois bytes ou menos — `BM` é o
  começo de muito arquivo de texto — e ninguém manda nenhum dos três.

### Quando os bytes discordam do nome

É o caso interessante, e ele ganhou resposta própria em vez de virar um ramo do
erro. **Não é erro de transferência.** As `NOTAS-DE-RELEASE.md` deste projeto
separam duas perguntas, e a seção «O que se recusa a receber», acima, diz que um
anexo alcançava só a primeira: *o arquivo chegou inteiro?* O hash respondeu sim
a ela. A segunda — *e ele é o que diz ser?* — passa a ter resposta para os
quatro formatos, e quando a resposta é não ela merece a frase dela.

O que a janela faz: escreve, na caixa do anexo, o que o arquivo **disse** que
era e o que os primeiros bytes dele **são**; diz que ele chegou inteiro e que o
hash fechou, para ninguém tentar de novo achando que foi a rede; e deixa o
arquivo onde estava, com o nome que veio, e o botão de salvar. **Não desenhar é
diferente de esconder**, e a diferença está na tela.

Duas coisas que o desenho recusa de propósito, e que são o mesmo erro em
direções opostas:

- **Não desenha como o que ele diz ser.** Óbvio, e é a regra.
- **Não desenha como o que ele por acaso é.** Um JPEG chamado `foto.png` não
  vira uma figura «porque no fim das contas era uma imagem mesmo». Desenhá-lo
  seria concluir que o nome não decide nada e que o arquivo de quem mandou
  decide tudo — que é o oráculo pelo lado avesso.

E o nome continua chegando como saiu: não renomeia, não corta extensão. Um
arquivo que mente é pior do que um que se apresenta.

### O teto da prévia, que não é o teto do arquivo

O teto por arquivo é uma fração do teto de disco que quem hospeda escolheu, e
protege o **disco dele**: no padrão de 1 GiB são 64 MiB. Desenhar é outra coisa.
Uma imagem de 64 MiB decodificada inteira se mede em gigabytes de pixels, e a
janela que tentasse pararia de responder — e a máquina que para é a de **quem
lê**, que não é a mesma máquina nem o mesmo recurso.

Então o limite da prévia é **4 MiB**, constante no cliente e não número que o
Dogma manda. Quanto de memória esta janela gasta não é de quem hospeda decidir.
Quatro mebibytes ficam acima de toda fotografia que um telefone produz e de toda
captura de tela que um portátil tira, que é para o que uma prévia serve.

A conferência acontece contra o tamanho **declarado no cabeçalho**, antes de um
byte do corpo ser lido, e o fluxo é cortado em vez de drenado — a mesma forma da
conferência do teto do Dogma, e pelo mesmo motivo: ler vinte megabytes e depois
decidir não olhar para eles custa o download inteiro.

O que esse número limita e o que ele **não** limita, dito em vez de suposto: ele
limita o download e os bytes que ficam na janela. Ele **não** limita os pixels
que a decodificação produz — para isso seria preciso ler a dimensão no cabeçalho
de cada um dos quatro formatos, que são quatro analisadores a mais, e não foi
construído. O tamanho **desenhado** é limitado na folha de estilo; o
decodificado não é.

### Quando a busca acontece

O anexo mora no Dogma. **Ver é baixar**, e essa frase é toda a decisão: se toda
imagem de uma Linha fosse buscada ao rolar a conversa, o teto de disco de quem
hospeda viraria banda de todo mundo — um giga de saída por vez que alguém
abrisse a Linha, multiplicado por quantas pessoas a abrissem.

Então a busca acontece **ao apertar um botão, e em nenhum outro momento**. O
botão só é oferecido quando o tipo alegado é um dos quatro e o arquivo cabe no
limite da prévia, e isso é conveniência e não é a regra: quem pedisse assim
mesmo receberia uma recusa enumerada, porque a regra é aplicada onde os bytes
estão.

O que voltou fica guardado por anexo, **inclusive quando é recusa**. A lista de
mensagens é reconstruída inteira a cada atualização, e sem isso rolar a conversa
apagaria toda figura já desenhada e pagaria o download de novo. E bytes que
discordaram de um nome não discordam menos na segunda tentativa: repetir a busca
gastaria a saída de quem hospeda para chegar à mesma conclusão.

### Prever não é abrir, e a linha fica escrita

É onde este trabalho podia errar feio, então a linha é desenhada em código e não
deixada ao bom senso:

- **Nada toca o sistema de arquivos.** Os bytes vêm para a memória e param ali.
  Não há arquivo, não há caminho, não há marca de quarentena a pôr porque não há
  o que marcar. Um cache em disco teria feito o ato de salvar acontecer sem
  ninguém pedir, e salvar é um ato de quem recebeu, num lugar que a pessoa
  escolheu.
- **Nada fora da janela é acionado.** Nenhum `open`, nenhum `shell`, nenhum
  gerenciador de arquivos. Continua não existindo `abrir_anexo`, e os dois
  guardas de ausência que cobravam isso continuam cobrando, agora sobre um
  frontend que tem uma figura dentro.
- **Salvar continua sendo o único verbo com destino**, e continua tendo a
  confirmação que diz em voz alta as duas coisas que este produto não promete.
  A prévia não ganhou confirmação nenhuma e não devia: ela não deixa nada em
  lugar nenhum.
- **Um anexo expirado não ganha botão nenhum.** Não há bytes para desenhar nem
  para salvar, e o que ele diz continua sendo que expirou.

### A CSP não mexeu, e isso era critério

`default-src 'self'`, e o ADR 0029 fez disto critério explícito: se desenhar
exigisse afrouxar a política, a resposta seria não. Não exigiu. A política já
trazia `img-src 'self' data:` e o `data:` é o caminho inteiro — a figura chega
como URI montado no Rust, e nada é buscado de host nenhum. Um guarda lê o
`tauri.conf.json` e reprova se `blob:`, `unsafe-inline`, `unsafe-eval` ou um
`https:` aparecerem ali.

### Nenhuma dependência nova, e a conta do base64

Zero crates. A conta, na forma que o ADR 0022 usou crate a crate, o 0026 usou
contando a árvore e a seção anterior usou contando três:

- **`base64`** seria um crate. Há **duas** versões dele na árvore hoje, ambas
  transitivas e nenhuma nossa para alcançar — depender de uma fixaria uma
  terceira aresta por vinte linhas de transformação definida por RFC. O
  codificador está escrito em `preview.rs`, **só codifica** — que é o total do
  que se precisa —, e é conferido contra os vetores da RFC 4648 mais os 256
  valores de byte. Um crate não se paga nesse tamanho.
- **Nenhuma biblioteca de imagem.** Quem decodifica é o motor do WebView, que já
  está no processo por causa do Tauri. Um `image` ou equivalente na árvore
  significaria um segundo decodificador lendo bytes de terceiro num processo que
  já tem um, e é o oposto de reduzir superfície.
- **`infer` ou `tree_magic`**, para farejar, seriam um crate para quatro
  assinaturas de no máximo doze bytes — e trariam junto centenas de formatos que
  este produto recusa desenhar de propósito. A lista curta é o recurso, não a
  limitação.

`cargo deny check` continua com `bans ok, licenses ok, sources ok`, porque nada
entrou.

### O que ficou de fora, e não é acidente

- **Documento não tem prévia.** O pedido dizia «imagem/documento», e PDF é a
  metade que não entrou: não é imagem, é um documento com um interpretador
  atrás, e desenhá-lo é entregar bytes de terceiro a esse interpretador. Ele
  continua sendo nome, tamanho e salvar.
- **Nenhuma miniatura, nenhum recorte, nenhuma rotação por EXIF.** Tudo isso é
  ler mais do arquivo de outra pessoa do que os doze bytes que decidem.
- **Nenhum limite de pixels**, pelo que a seção do teto já diz.
- **Som e vídeo não tocam.** Um `<audio>` ou um `<video>` é o mesmo argumento do
  decodificador com um contêiner por cima, e não foi pedido.

### O que os guardas cobram

O que mais importa está encenado e não lido: `crates/seele-conformance/tests/anexos.rs`
sobe um Dogma de verdade, escreve um arquivo em disco cujos bytes são de um
JPEG, manda-o pela rede chamado `foto.png` e alegado `image/png`, busca-o de
volta e cobra que o veredito seja discordância — **e que não seja desenho, em
nenhum dos dois sentidos**. O mesmo com um executável chamado `gatinho.png`. O
ramo que desenha é encenado igual, e o arquivo acima do limite da prévia também,
com a conexão continuando viva depois de o fluxo ter sido cortado.

Do lado da página, os guardas cobram propriedades e não nomes: que
`prever_anexo` seja chamado de um lugar só e que esse lugar seja o ouvinte de
clique; que redesenhar a conversa não busque nada; que a página não monte
`data:` nem leia `declared_type`; que os quatro tipos não estejam escritos duas
vezes; que a frase da discordância diga «chegou inteiro»; que a recusa caia numa
região que a folha de estilo não esconde; e que o caminho da prévia não alcance
`salvar_anexo`, `destino` nem `armarAto`.
