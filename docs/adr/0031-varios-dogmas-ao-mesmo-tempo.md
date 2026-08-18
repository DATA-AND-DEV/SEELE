# ADR 0031 — Vários Dogmas ao mesmo tempo: a sessão é do Dogma, o microfone é da máquina

**Estado:** proposto
**Data:** 2026-08-18

Pedido do dono, nas palavras dele: **«Botão + funcionando, para que um usuário
possa se conectar a vários Dogmas ao mesmo tempo.»**

Parece um botão e não é. A pergunta que ele obriga a responder é **o que
pertence a um Dogma e o que pertence a esta máquina**, e essa pergunta hoje não
tem resposta escrita em lugar nenhum — porque com uma sessão por vez as duas
coisas são a mesma coisa e ninguém precisou separá-las. O ADR 0032, sobre
personalização de um Dogma, depende da mesma resposta e a cita daqui.

## Contexto

### O `+` está na tela, desabilitado, e o motivo está escrito ao lado dele

`apps/seele-app/ui/index.html:906` tem o botão, com `disabled` e com o `title`
dizendo por quê: *«este produto mantém um Plug por vez: para trocar de Dogma,
use DESCONECTAR»*. O comentário acima dele conta a história inteira — no comp v2
o `+` acumulava «entrar noutro Dogma» e «sair deste», um teste entre duas
máquinas mostrou que a pessoa não achava como sair, o `DESCONECTAR` virou texto
no cabeçalho, e o que voltou ao `+` foi só a metade que ele nunca teve. A
moldura ficou; a promessa, não.

O comentário da trilha, logo acima (`index.html:873-884`), já anota o que este
ADR vem resolver: *«A trilha fica com o que existe — quando o segundo Plug
existir, ela já é o lugar dele.»*

### O que existe hoje, e que condiciona todo o desenho abaixo

Isto foi conferido no código antes de qualquer linha ser desenhada, e três dos
achados mudaram o desenho.

- **Uma sessão por vez, e o guarda é explícito.**
  `apps/seele-app/src/main.rs:49` guarda `plug: Mutex<Option<Arc<Plug>>>`, e
  `connect` recusa com `PlugError::AlreadyConnected` quando já há uma
  (`main.rs:168-170`).
- **O `Snapshot` é singular**, e descreve *o* Dogma:
  `crates/seele-ffi/src/types.rs:546` em diante — `link`, `pattern`, `dogma`,
  `me`, `nickname`, `cages`, `lines`, `messages_revision`, `telemetry`, `notice`,
  e os campos de áudio e de permissão. **A janela consome 23 dos 24 campos** (só
  `me` não é lido), em **28 funções de desenho** espalhadas por cinco módulos, e
  há ~240 menções a `snapshot` na pasta `ui/`.
- **O terminal não o consome, e isto foi conferido em vez de suposto.**
  `grep -r Snapshot crates/seele-tui/` devolve **zero**: o `plug` não depende de
  `seele-ffi` — só de `seele-core` e `seele-server`, com o comentário do ADR 0002
  no próprio `Cargo.toml` («a shell sees `seele-core` and nothing else»). Mudar o
  `Snapshot` **não custa nada ao terminal**, e a primeira versão deste ADR dizia o
  contrário. Fica registrado porque a suposição era plausível e teria mudado a
  decisão da seção «A forma do dado».
- **Cinco fatias de sessão num `Session` que tem uma só.** `main.rs:48-91`: além
  do `plug`, também `busca` e `convite` são `Mutex<Option<…>>` de vaga única e são
  tão da sessão quanto ele. `hospedagem` e `atualizacao` são da máquina.
- **E há 23 variáveis de módulo no JavaScript que já são estado de sessão** sem
  nunca terem sido chamadas assim — `linhaAberta`, `mensagens`,
  `revisaoDesenhada`, `anexoPendente`, `alvoDoDogma`, `salasDoDogma`,
  `casamentosPorMensagem` e as outras, em cinco arquivos. **O endereço do
  servidor não está no `Snapshot`**: ele sobrevive só no global `alvoDoDogma`
  (`ui/tela-auth.js:74`, com o comentário dizendo exatamente isso).
- **Não há roteador nem máquina de estados.** Seis telas trocadas por
  `elemento.hidden`, e o `base.js:126` é explícito: «Nenhuma delas decide qual é a
  tela seguinte». Não existe hoje o conceito de «sessão corrente» em que rotear.
- **Um canal de eventos, e o evento não diz de quem é.** `EVENT_CHANNEL`
  (`main.rs:36`) é um só, de propósito — «o payload já diz qual `Event` é» — e a
  `Bridge` (`main.rs:110-116`) emite o `Event` cru. **Nada nele carrega qual
  sessão o produziu.** Com duas sessões isto não quebra: ele atribui a mensagem
  do Dogma B ao Dogma A, calado, e é o defeito mais barato de introduzir e mais
  caro de achar desta página inteira.
- **O `Plug` é fino, e o que ele pendura não é.** São dois campos
  (`crates/seele-ffi/src/lib.rs:450-453`), e o único que toca hardware é
  `shared.voice` (`lib.rs:289`) — que segura um `seele_core::voice::Voice`, que
  segura um `AudioIo` com **dois `cpal::Stream` vivos** e a thread
  `seele-voice` com mixer, codificador, decodificadores, jitter buffers e portão.
- **O áudio abre na conexão, e não na entrada no Cage.** É o achado que mais
  mexeu neste desenho. `lib.rs:1507-1531` chama `Voice::start_preferring` dentro
  do `drive`, antes do laço de comandos; `insert_plug` não toca em áudio
  nenhum (`lib.rs:565-567`, `lib.rs:1787-1795`), e quem decide se a voz sai é o
  portão, a cada 20 ms (`crates/seele-core/src/voice.rs:834-857`). Em outras
  palavras: **hoje conectar já abre o microfone, mesmo que a pessoa nunca entre
  num Cage.** Três conexões abririam três.
- **Trocar a sessão embaixo de um caminho de voz vivo já é possível, e já está
  construído.** `Voice::reopen(media, ssrc)` (`lib.rs:1587-1626`) foi feito para
  reconexão, e carrega A.T. Field, isolamento total, modo de voz, tecla segurada
  e os ganhos por falante (`voice.rs:603-625`). É exatamente a operação «este
  microfone passa a falar naquela outra sessão», e ela existe hoje por outro
  motivo.
- **`set_talking` não passa pela fila de comandos, de propósito** — `lib.rs:898-908`:
  *«it has to take effect on the next 20 ms frame, and a round trip through the
  driver thread would put a queue between a key and a microphone.»* Com dois
  `Plug`, essa chamada deixa de ter referente: não há «o» microfone.
- **A barra de espaço é da janela, não do sistema.**
  `apps/seele-app/ui/tela-sessao.js:1689,1714-1723` são `keydown`, `keyup` e um
  `blur` que solta a fala — *«uma janela que perde o foco com o microfone aberto
  é um microfone esquecido»*. Não há `tauri-plugin-global-shortcut` na árvore.
- **As preferências desta máquina têm dois campos.**
  `crates/seele-core/src/preferences.rs:56-61`: `capture` e `playback`. O
  cabeçalho do módulo já traça a fronteira que este ADR precisa: *«none of it
  follows the pilot to another computer, and every one of them is about the
  hardware in front of the person rather than about the Dogma»*.
- **A lista de Dogmas visitados já existe.** `crates/seele-core/src/conhecidos.rs`
  guarda endereço, apelido e último Cage por Dogma, e é o que alimenta a tela de
  entrada das duas cascas.
- **A identidade é uma só por máquina.** ADR 0004 e 0017: `identity.key`, 32
  bytes, modo `0600`, em `$SEELE_HOME`; o apelido é preso à chave **dentro de
  cada Dogma**, por `register_or_find`.
- **A portaria é da máquina que hospeda.** ADR 0030, e `hospedagem` no
  `main.rs:54` é um `Option` — este app hospeda no máximo um Dogma, e isto não
  muda aqui.
- **Um operador de outro Dogma pode inserir o seu plug.**
  `crates/seele-server/src/session.rs:1220-1230`: `MovePilot` transmite
  `PilotMoved` sem exigir que o piloto já esteja num Cage. É a única forma de
  alguém remoto mexer no que este desenho considera hardware desta máquina, e ela
  tem seção própria abaixo.

## Decisão

**Várias sessões ao mesmo tempo; um caminho de voz. A sessão é do Dogma. O
microfone, a saída, a tecla e a identidade são desta máquina — e o caminho de
voz mora no Cage em que a pessoa está, que é um só, somando todos os Dogmas.**

### O corte, numa tabela, porque é ele que decide o resto

| | do Dogma (uma por sessão) | desta máquina (uma, sempre) |
|---|---|---|
| conversa | Cages, Linhas, histórico, roster, telemetria, veredito da chave | — |
| identidade | o apelido, como aquele Dogma o resolveu; as permissões | a chave, e a impressão digital dela |
| voz | em qual Cage o plug está | **o microfone, a saída, o modo, o A.T. Field, o isolamento total, a tecla** |
| aparência | o nome, e a placa (ADR 0032) | a palheta, o MOD (ADR 0029), a janela |
| infraestrutura | a conexão, o socket, os pins daquele endereço | a lista de visitados, o atualizador, a portaria de quem hospeda |

Tudo o mais neste documento é consequência desta tabela, e a linha que custa é a
da voz.

### Voz: não dá para estar em jaula de dois Dogmas ao mesmo tempo

**A resposta é não**, e ela não é uma limitação de implementação que um dia se
supera. É a resposta certa, e tem três motivos que se somam:

**1. O microfone é um, e a pessoa é uma.** Estar audível em dois Cages ao mesmo
tempo significa que toda palavra vai para dois conjuntos de gente, e que no
instante de falar não há como olhar para a tela e saber quem está ouvindo. É a
falha mais cara que este produto tem — `specs/03-audio.md` escolheu PTT como
padrão justamente porque *«um cliente que transmite uma sala por engano é a falha
mais cara das duas»*, e o ADR 0016 repetiu o argumento ao recusar o VAD como
padrão. Voz concorrente é essa falha promovida a recurso.

**2. A barra de espaço deixaria de ter referente.** Ela é uma tecla, na janela,
sem alvo — `tela-sessao.js:1714` chama `segurarFala(true)` e pronto. Com dois
Cages abertos, «segurar espaço» teria de significar um dos dois, e a escolha
seria feita por qual painel está em foco: um microfone cujo destino depende de
onde o cursor estava. Não há desenho bom para isso, e o ADR 0016 já enfrentou a
mesma classe de problema em terminal e concluiu que **o estado tem de ser
visível e explícito, nunca um modo escondido**.

**3. O orçamento de latência é medido, e é de um caminho.** O ADR 0009 tem
orçamento boca-a-ouvido, e o ADR 0028 já gastou 21 ms dele numa reserva de anel
que precisou de um ADR próprio para ser justificada. Dois caminhos de voz são
dois `cpal::Stream` de entrada, dois codificadores, dois mixers e duas threads de
tempo real, num produto que dimensiona o Dogma em 1 vCPU e conta crate a crate.

E há um quarto, que é de higiene e que aparece de graça: **hoje o microfone abre
na conexão.** Três Dogmas conectados abririam três `AudioIo` antes de alguém
dizer uma palavra — três aberturas do mesmo dispositivo, três luzinhas de
microfone acesas, e nenhuma delas transmitindo nada.

### Então o caminho de voz sai do `Plug` e passa a ser da máquina

A consequência de decidir «um só» é onde ele mora.

**O caminho de voz é aberto na entrada no primeiro Cage, e não na conexão.** É
mudança de comportamento e é boa por si: quem conecta e não fala não abre
microfone nenhum, o que hoje não é verdade. O que se perde é uma latência que
hoje já está paga na conexão — abrir dispositivo custa, e passa a custar no
primeiro «entrar». Em troca, a mensagem «não consegui abrir o microfone» passa a
chegar **quando a pessoa quis falar**, que é quando ela significa alguma coisa;
hoje ela chega no meio de uma conexão e é um aviso sobre nada
(`lib.rs:1526-1529`: *«No microphone is not a reason to have no session»* —
continua verdade, e passa a ser verdade no lugar certo).

**Entrar num Cage de outro Dogma é tomar o microfone, e o Cage anterior é
ejetado.** Um ato, não dois; e o Cage que se deixou é **nomeado** no aviso, com o
Dogma dele. Não há confirmação: perguntar «tem certeza?» no caso normal é o
escape que o ADR 0029 recusou por escrito — *«um escape existe para ser clicado
uma vez e esquecido»* —, e aqui o caso normal é justamente este.

**E a operação já existe.** `Voice::reopen(media, ssrc)` repõe um caminho de voz
vivo sobre outra sessão carregando A.T. Field, isolamento, modo, tecla segurada e
ganhos. Ele foi construído para reconexão; mudar de Dogma é a mesma chamada com
outro `media` e outro `ssrc`. **Este é o motivo de a decisão ser barata onde ela
parecia cara**: o que faltava não era o mecanismo, era o dono dele.

**O que fica concorrente é tudo o que não é voz.** As três sessões recebem texto,
presença, moderação e telemetria ao mesmo tempo; escrever numa Linha de qualquer
uma delas funciona sem trocar nada de lugar. **A exclusividade é do microfone, e
por isso o escopo dela é o Cage — não o Dogma.**

### Um plug que outra pessoa insere não toma o microfone

`MovePilot` transmite `PilotMoved` sem exigir que o piloto já tenha um plug
inserido (`session.rs:1220-1230`). Ou seja: um Comandante de um Dogma que está em
segundo plano pode pôr o seu plug num Cage de lá, sem que você tenha tocado em
nada.

**A regra:** o servidor decide quem está na sala; **esta máquina decide para onde
o microfone dela vai.** Um plug inserido por outra pessoa entra no roster daquele
Dogma e **não** reivindica o caminho de voz. A placa daquele Dogma mostra o plug
inserido e mostra que o microfone não está lá; quem decide mudar é quem está na
frente do computador.

O contrário seria uma pessoa remota tirar o seu microfone da conversa em que você
está e pô-lo na dela. Nenhuma permissão deste produto deveria conseguir isso, e
`Permission::MovePilot` foi escrita para arrumar uma sala, não para tomar
hardware.

### A identidade: a mesma chave nos três, e o que isso custa

`identity.key` é um arquivo, e a conexão prova a posse dele. Em três Dogmas é a
**mesma chave**, e três coisas decorrem:

**O apelido é por Dogma, e sempre foi.** `register_or_find` roda no CASPER de
cada Dogma, e o ADR 0017 prende o apelido à chave **dentro** daquele Dogma.
Então ser outra pessoa noutro lugar já funciona, para o nome: `ayanami` num,
`Rafael` noutro, e nada no cliente impede. O `conhecidos` já guarda um apelido
por endereço, exatamente para isso.

**A chave, não.** A mesma impressão digital aparece nos três — inclusive nas três
portarias do ADR 0030, cujo cartão mostra o SHA-256 em primeiro lugar e por
extenso, porque é *«a única coisa nesse cartão que outra pessoa não pode
escolher»*. Dois Dogmas que comparem anotações sabem que foi a mesma máquina.
Isso já era verdade em série e passa a ser verdade em paralelo; a diferença é que
antes ninguém tinha motivo para descrever o produto como «uma identidade por
pessoa» e agora tem.

**Chave por Dogma fica de fora, e o motivo não é preguiça.** Já existe uma forma
de ter duas identidades — `$SEELE_HOME`, que o ADR 0017 pôs em primeiro na ordem
justamente para que *«dois clientes na mesma máquina possam ser dois pilotos»* —
e ela é por processo, então não compõe com uma janela segurando três sessões.
Fazer a escolha por Dogma dentro de uma janela é construir gerência de contas: N
chaves para guardar, N para perder, e o ADR 0017 já registra que perder o arquivo
é perder o apelido sem recuperação. A decisão de chave com senha foi empurrada
para M5 naquele ADR, de propósito, e este não a antecipa de carona.

### Recursos que são um só, um por um

- **Microfone e saída** — da máquina. Continuam em `preferences`, continuam
  escolhidos no Terminal Dogma, e passam a valer para o caminho de voz, que é um.
  **Nada aqui vira por Dogma**, e a razão está no próprio cabeçalho do módulo: são
  perguntas sobre o hardware na frente da pessoa, não sobre o Dogma.
- **Modo de voz (PTT ou VAD), A.T. Field, isolamento total** — da máquina, pela
  mesma razão: são propriedades de como aquele microfone abre e de se aquele
  alto-falante toca. Hoje eles moram no `Snapshot`, que é por sessão; **eles saem
  de lá**, e é a mudança de tipo mais visível deste ADR.
- **A tecla** — da máquina, e continua da janela: não há atalho global, e o
  `blur` continua soltando a fala. Com voz exclusiva, «espaço» tem exatamente um
  significado, sempre. **Esta é a propriedade que a exclusividade compra**, e é o
  argumento mais concreto a favor dela.
- **Volume por falante** (`set_volume(nickname, percent)`) — por sessão, porque
  apelido é por Dogma. Um `ayanami` num Dogma não é o `ayanami` do outro.
- **O atualizador** (ADR 0026) — da máquina, um botão, inalterado. O que muda é o
  custo de apertá-lo: instalar derruba **três** conversas em vez de uma, e a tela
  de instalação passa a ter de dizer quantas.
- **O MOD** (ADR 0029) — da máquina, um por vez, e nenhum Dogma o escolhe.
- **A portaria** (ADR 0030) — da máquina que hospeda, e este app hospeda um
  Dogma. Ela **não** se multiplica com as sessões, e isso é sorte de desenho: os
  sete comandos dela falam direto com o CASPER local e nunca pelo fio, então não
  há «a portaria de qual sessão» a responder.

### Onde aparece o que acontece num Dogma que não está na frente

Esta é a pergunta que o pedido embutiu sem dizer: um Dogma aprovando alguém
enquanto você fala noutro.

**A faixa de alerta é da sessão que está na frente, e de mais nenhuma.** Tudo o
que acontece numa sessão em segundo plano vai para **a placa dela na trilha**, e
nunca para o meio da tela. O motivo é o do ADR 0029, dito ali sobre tema e válido
igual aqui: uma tela que muda porque algo aconteceu num lugar para onde a pessoa
não está olhando é o movimento menos diagnóstico que este produto poderia ter.
`specs/07` diz que movimento é diagnóstico, e a exceção nomeada já foi gasta pela
varredura.

Três coisas seguem dessa regra:

- **Uma queda de enlace em segundo plano é vermelha na placa daquele Dogma**, e
  não na banda. Vermelho ali é o vermelho certo: é queda, que é o que o token é
  reservado para dizer. O ADR 0032 fecha a outra metade — **estado vence
  identidade na placa** —, e é por isso que os dois ADRs precisam concordar sobre
  o mesmo elemento de 56 px.
- **A portaria conta na porta, não na trilha.** Ela é da máquina que hospeda e já
  tem o botão que conta pendentes a cada cinco segundos (pendência 23). Nada
  disto a move.
- **Menção e aviso de operador em segundo plano** ganham marca na placa, e a
  frase inteira só quando a pessoa vai lá. É o mesmo corte que o `messages_revision`
  já ensinou: quem não está olhando não precisa do conteúdo, precisa de saber que
  mudou.

### A forma do dado: `Snapshot` não vira plural

O `Snapshot` fica **exatamente como é: um Dogma**. O que nasce é uma camada
acima dele — a lista de sessões, qual é a atual, e o bloco do que é desta
máquina.

Três razões, e a terceira é a que decide:

1. **`messages_revision` sobrevive.** Aquele campo existe porque carregar a
   conversa no `Snapshot` custava um clone de cada apelido e cada corpo a cada
   quadro de interface, duas vezes por segundo — e há um teste que reprova se
   `Vec<Message>` voltar (`types.rs:1110-1141`). Um `Snapshot` plural
   multiplicaria esse custo por sessão em vez de resolvê-lo.
2. **A leitura por sessão continua barata.** A casca desenha uma sessão por vez e
   lê as outras como placa; ler três telas inteiras a cada 500 ms para desenhar
   uma seria pagar três vezes pelo que se vê uma.
3. **As 28 funções de desenho continuam recebendo o que já recebem.** Elas tomam
   um `snapshot` e desenham um Dogma, e é o que continuam fazendo. Pluralizar o
   tipo obrigaria cada uma a escolher de qual sessão fala — que é justamente o
   trabalho que a camada de cima existe para fazer **uma vez**.

**O que o `Snapshot` perde** são os campos que a tabela do corte declarou da
máquina: `at_field`, `total_isolation`, `voice_mode`, `capture`, `playback`,
`audio_available`, `speaking`. Eles vão para o bloco de máquina.

**E isso não custa nada ao terminal**, ao contrário do que se supôs ao começar
este documento: o `crates/seele-tui` não menciona `Snapshot` uma vez e nem sequer
depende do `seele-ffi`. A regra de dependência do ADR 0002 pagou por si aqui, e
vale dizer em voz alta que ela pagou: **toda a mudança de tipo desta página é da
janela**, e não porque alguém teve o cuidado de a conter agora.

**E todo evento passa a dizer de qual sessão é.** Não há alternativa: o canal é
um, o `Event` é o payload, e sem isso a mensagem do Dogma B é desenhada na Linha
do Dogma A. É o defeito silencioso desta página inteira, e o conserto é um campo.

## O tamanho da mudança, sem estimar para baixo

O pedido é «um botão». O trabalho é o seguinte, e nenhuma linha disto é
opcional:

- **A ponte.** `apps/seele-app/src/main.rs` tem **51 `#[tauri::command]`**, dos
  quais **23 resolvem `session.plug()?`** e mais quatro alcançam o `Plug` de outra
  forma — `set_talking`, `escolher_microfone`, `escolher_saida` e o próprio
  `connect`, que é onde a regra de uma sessão é aplicada. Cada um passa a precisar
  saber **de qual Dogma** fala. A mudança é mecânica em cada um e nada mecânica no
  conjunto: o parâmetro novo tem de chegar de algum lugar na página, e a página
  tem de saber a qual sessão pertence o controle que o disparou.
- **O evento.** Um campo novo em tudo o que atravessa `EVENT_CHANNEL`, e a
  `Bridge` deixa de ser um `emit` cru. Hoje há **dois ouvintes no mesmo canal**
  (`ui/tela-sessao.js:1727` e `ui/tela-chamada.js:637`), e nenhum tem como
  filtrar; o primeiro termina num `atualizar()` pelado, que quer dizer «mudou
  alguma coisa em algum lugar, releia *a* tela».
- **A casca.** `apps/seele-app/ui/` são ~13 mil linhas; só `tela-sessao.js` são
  1771 e `tela-sessao.css` são 1555. São **28 funções de desenho** e **23 de 24
  campos** do `Snapshot` lidos. Toda leitura que hoje diz «o Dogma» passa a dizer
  «este Dogma».
- **As 23 variáveis de módulo.** Cada uma delas vira um mapa por sessão, ou a
  Linha aberta do Dogma A reaparece no Dogma B. É o pedaço mais fácil de
  subestimar da lista, porque nenhuma delas se chama «estado de sessão» hoje.
- **O roteamento, que não existe.** Seis telas por `hidden` e nenhum conceito de
  sessão corrente. Ele não precisa virar um framework — o ADR 0019 decidiu que não
  há framework —, mas precisa virar **um lugar**, e hoje não há lugar nenhum.
- **A trilha.** É o menor pedaço do trabalho e é o único que o pedido descreve.
- **A voz.** Tirar o caminho de voz do `Plug`, abri-lo na entrada no Cage, e
  fazer a troca entre sessões pelo `reopen` que já existe. É o pedaço mais
  delicado e o de menor volume.
- **Os testes.** `apps/seele-app/tests/frontend.rs` tem 105 testes, e o que cobra
  a promessa de hoje tem nome:
  **`the_add_dogma_button_promises_nothing_this_product_can_do`**
  (`frontend.rs:2150-2183`). Ele exige que `#trilha-adicionar` esteja `disabled`,
  que tenha `title` e `aria-label`, e — a assertiva que reprova no primeiro
  minuto deste trabalho — que **nenhum script mencione `$("trilha-adicionar")`**.
  Ele não é obstáculo: é a moldura sendo cobrada como moldura, exatamente como
  foi escrita. Quando o `+` funcionar, ele vira o teste do contrário. Junto vão
  `the_button_with_no_command_behind_it_cannot_be_pressed…` (a metade do
  `#bateria-forcar`) e as frases que mandam trocar de Dogma pelo DESCONECTAR.
- **O que a máquina passa a gastar por sessão:** uma conexão QUIC, um socket, os
  temporizadores de enlace e um laço de leitura de estado a cada 500 ms — a rede
  de segurança que o ADR 0027 descobriu escondendo um defeito por meses. Três
  Dogmas é três de cada.

**O que este ADR não muda, e é bom que não mude:** o `seele-proto` inteiro, o
`seele-server`, o `seele-core` fora da voz, o esquema do banco, a portaria, o
atualizador e a palheta. **Nenhum verbo novo de protocolo.** Toda a mudança é do
lado do cliente, e isso é o que torna o custo grande em vez de perigoso.

## Alternativas consideradas

1. **Uma janela por Dogma.** É o que o Tauri torna quase de graça, e é a resposta
   mais barata em código. Recusada porque **duplica exatamente os recursos que são
   um só**: duas janelas são dois ouvintes de barra de espaço, com dois `blur`
   que soltam a fala um do outro; dois `preferences` lidos e escritos sem que um
   saiba do outro; dois microfones abertos. E porque não responde ao pedido: o
   `+` está numa trilha, e trilha é uma coluna de uma janela.

2. **Voz simultânea nos dois Dogmas, mixada na mesma saída.** É construível — o
   mixer existe e sabe somar fontes. Recusada pelos três motivos da seção de voz,
   e por um quarto que é da escuta: duas salas somadas num par de fones produzem
   um fluxo em que não dá para dizer de qual sala uma voz veio, e este produto tem
   regra escrita contra informação que viaja por um canal só.

3. **Uma sessão por vez, com troca rápida.** Guardar as credenciais e reconectar
   num atalho. É pequeno, e é quase o que existe. Recusada porque **não entrega o
   que o pedido pede**: o valor de estar em três Dogmas é receber dos três,
   inclusive daqueles para onde não se está olhando. Uma troca rápida entrega a
   conveniência de digitar menos e nenhuma das outras.

4. **Um processo por Dogma, com `$SEELE_HOME` diferente.** Funciona hoje, é o que
   o ADR 0017 previu para testar as duas pontas, e dá identidade por Dogma de
   graça. Recusada como resposta ao pedido: são N janelas, N caminhos de
   atualização e N microfones — os mesmos problemas da alternativa 1, com uma
   fronteira de processo no meio para tornar impossível o coordenamento que a
   alternativa 1 pelo menos poderia ter. O `+` viraria um lançador de programas.

5. **Voz exclusiva por Dogma, e não por Cage** — conectar-se a B derrubaria a voz
   de A no ato de conectar. Recusada porque conectar não é falar: com o áudio
   saindo da conexão, entrar num Dogma para ler uma Linha não tem por que custar a
   conversa em que a pessoa está. A exclusividade tem de morar no recurso que é
   exclusivo, e ele é o microfone.

## Consequências

- **O `+` passa a fazer uma coisa só, que é o que o comp v3 corrigiu nele**, e o
  `DESCONECTAR` continua onde está. O `title` que explica a limitação sai, e o
  teste que o cobra sai com ele.
- **O microfone deixa de abrir na conexão.** Quem conecta e não fala não abre
  dispositivo nenhum. É melhoria de privacidade e de recurso que cai de graça, e é
  também uma mudança de comportamento que alguém vai notar.
- **`Snapshot` perde sete campos para um bloco de máquina**, e **nada disso
  alcança o terminal**, que não depende do `seele-ffi`. A regra de dependência do
  ADR 0002 é quem garante isso, e é a segunda vez que ela se paga sem ter sido
  invocada.
- **Todo evento carrega a sessão**, e a `Bridge` deixa de ser um repasse.
- **Nada no protocolo muda**, nada no servidor muda, e nenhuma permissão nova
  nasce. É a razão de o custo deste ADR ser todo de casca.
- **Instalar uma atualização passa a derrubar N conversas**, e a tela precisa
  dizer quantas.
- **Três conexões são três vezes o custo de enlace ocioso** — temporizadores,
  keepalive, e o laço de estado de 500 ms por sessão. Num notebook em bateria isso
  se nota, e não há medida aqui porque medir é trabalho próprio.
- **A `pendência 11`** — reconectar rápido pode esvaziar o roster do Cage — passa
  a ter mais ocasiões de acontecer, porque há mais enlaces em pé ao mesmo tempo.
  Este ADR não a piora por desenho e não a conserta.

## O que fica sem saída

**Uma chave para todos os Dogmas.** Não há pseudonímia entre Dogmas neste
produto, e nunca houve; com três sessões vivas, a mesma impressão digital está em
três portarias ao mesmo tempo. A saída é uma segunda identidade, que é gerência
de chaves, que o ADR 0017 empurrou para M5 com o motivo escrito.

**Uma pessoa só fala num lugar de cada vez, e às vezes ela quer dois.** «Estou na
call do trabalho e quero ouvir os amigos» é um pedido legítimo, e a resposta deste
ADR é não. Ela é a resposta certa pelos três motivos da seção de voz, e continua
sendo um não.

**A tela em segundo plano não conta o que aconteceu, só que aconteceu.** Uma
marca numa placa é menos do que uma frase, e há casos em que a frase importava —
um aviso de operador, por exemplo. A alternativa era deixar um Dogma para onde
ninguém está olhando tomar a tela, e essa é pior. Não há terceira.

**O terminal continua com um Dogma, e é o segundo ADR seguido em que ele fica
para trás.** Depois do 0029, que o deixou com a palheta congelada. A diferença é
que aqui ficar para trás não custa nada — ele não depende do `seele-ffi`, então o
tipo muda sem o alcançar — e que ele **já tem uma resposta**: dois terminais e
dois `$SEELE_HOME` são dois Dogmas, com duas identidades de brinde, que é
justamente o que a janela não consegue oferecer. É a resposta do terminal e não é
a mesma coisa: ela não compartilha nem o microfone, nem a lista de visitados, nem
a trilha. Mas duas cascas que resolvem o mesmo problema por caminhos que não se
encontram é uma divergência, e este é o lugar de anotá-la antes que seja uma
descoberta.

**Não há número para quantos Dogmas cabem.** Nem teto, nem medida. Cada sessão
custa uma conexão e um laço de estado, e o produto não sabe dizer se dez é muito.
O ADR 0027 escolheu ter um teto e escreveu que ter um número no dia um era a
propriedade inteira daquela decisão; aqui não há número, e dizer isso é melhor do
que inventar um.

## Custo de reverter

**Alto, e é honesto dizer antes de começar.** Não porque haja formato em disco a
migrar — não há: `conhecidos` e `preferences` continuam iguais, o protocolo não
muda, o banco não muda. É alto porque a mudança é **de forma**, espalhada por
toda a casca: voltar atrás é refazer o caminho inteiro no sentido contrário.

**A parte da voz reverte barato**, e essa propriedade não foi acidente: um
caminho de voz que é da máquina e se repõe por `reopen` já é a forma que a
reconexão usa. Uma sessão só é o caso `N = 1` do mesmo código, e não um caminho
separado.
