# ADR 0035 — O código deixa de falar Evangelion

**Estado:** aceito
**Data:** 2026-08-25

O [ADR 0033](0033-o-vocabulario-sai-da-interface-a-estetica-fica.md) tirou o
vocabulário da **tela** e escreveu, numa seção chamada "O que este ADR
explicitamente NÃO muda":

> **Nomes de tipo, campo, função, variante e crate.** `Cage`, `CageId`, `Dogma`,
> `Pilot`, `Line`, `SyncRatio`, `at_field`, `insert_plug`, `Pattern::Blue`
> continuam exatamente como estão. O código não é interface. Renomear tipo é
> outro trabalho, com outro custo, e não é este.

Este ADR é aquele outro trabalho.

## Contexto

O pedido do dono foi direto: tirar do sistema tudo que faz correlação com a
obra. O levantamento mostrou que a tela já estava quase limpa — restava **uma**
citação em texto visível, o `<h1>TERMINAL DOGMA</h1>` da tela de configuração —
e que os 2.052 `Dogma`, 1.426 `Cage`, 671 `Pilot` e 519 `Line` estavam em código,
comentário e documento.

O [ADR 0034](0034-a-marca-abandona-as-duas-citacoes-do-anime.md) já havia feito o
mesmo movimento na imagem, e a frase dele vale aqui: *"o argumento de que uma
forma desenhada não é vocabulário era verdadeiro e insuficiente"*. O mesmo se
aplica a "o código não é interface": é verdadeiro, e insuficiente quando o
objetivo é que o produto pare de citar a obra.

**`SEELE` fica.** É o nome do produto, nunca foi citação — o 0034 já tirou da
marca o katakana `ゼーレ` e a silhueta do plug de entrada. Decisão do dono, e é a
única coisa que permanece.

## Decisão

| era | é | onde |
|---|---|---|
| `Dogma` | `Server` | tipo, módulo, permissão |
| `Server` (o daemon em execução) | `Daemon` | tipo |
| `Cage` | `VoiceRoom` | tipo, tabela, permissão |
| `Pilot` | `Person` | tipo, tabela, permissão |
| `Line` | `Channel` | tipo, tabela, coluna, índice, permissão |
| `Casper` · `Melchior` · `Balthasar` | `Persistence` · `Permissions` · `Media` | módulo, subsistema |
| `at_field` | `muted` | campo, mensagem de protocolo |
| `SyncRatio` · `SyncBand` | `Signal` · `SignalBand` | tipo, módulo |
| `Pattern` (`PADRÃO: AZUL`) | `LinkTrust` | tipo |
| `Plug` · `InsertPlug` · `EjectPlug` | `Connection` · `EnterVoiceRoom` · `LeaveVoiceRoom` | tipo, permissão |

### Três nomes saíram do mapa por colisão, e cada uma ensinou algo

**`Dogma` → `Server` exigiu renomear o `Server` que existia.** Ele estava em
`seele-server/src/lib.rs` documentado como *"A running Dogma"*, enquanto
`struct Dogma` vivia em `dogma.rs`: dois conceitos no mesmo crate, o que se
configura e o que roda. O que roda virou `Daemon`, porque o binário é literalmente
`seeled`.

**`Cage` não virou `Room`.** Era o mapa inicial, e teria exigido `Room` →
`ServerState` junto — 940 `room` minúsculos, mais que os 1.426 do próprio `Cage`,
e parte deles prosa inglesa legítima ("a room full of people"). `VoiceRoom` não
colide com nada e diz o que a tela já dizia. `Room` significando "tudo o que este
cliente sabe do servidor" continua sendo um nome ruim, e continua disponível como
trabalho próprio: ele não tem nada de Evangelion e não precisa ser refém deste.

**`Pattern` não virou `LinkState` nem `Trust`.** Os dois já existiam — o primeiro
é o estado do enlace com bateria interna, o segundo é o veredito do TOFU.
`LinkTrust` diz o que a coisa é.

### O banco: cinco migrações, e a regra que quase foi quebrada

O cabeçalho de `persistence/schema.rs` proíbe, em letras próprias:

> **Append only once shipped.** Editar uma migração que já chegou a banco de
> verdade significa duas instalações reivindicando a mesma versão com formas
> diferentes.

Uma varredura de renomeação editou a migração 1 por acidente: trocou o papel
semeado `'Pilot'` e as permissões em JSON `"MovePilot"` **dentro do texto já
entregue**. Um banco de beta tester guarda `"MovePilot"`; o código passaria a
procurar `"MovePerson"`, e a permissão de mover alguém sumiria **sem erro
nenhum** — não em teste, só em uso. A migração 1 foi restaurada e o rename foi
para onde pertence: as migrações **5 a 9**, que renomeiam tabela, coluna e índice
e reescrevem os nomes de permissão dentro dos arrays JSON de `roles`.

Nome de permissão é **texto gravado**, e essa é a lição que se repetiu cinco
vezes: o rename do enum em Rust não chega ao banco sozinho.

### O que **não** foi reescrito, e por quê

**Os ADRs anteriores, os planos executados em `docs/superpowers/`, os comps em
`design/` e as migrações já entregues.** O critério: *reescreve o que descreve o
sistema de hoje, preserva o que registra a decisão de ontem*. Reescrever o ADR
0033 — o documento que explica por que `MELCHIOR` ficava — o tornaria incoerente,
e este repositório trata registro fechado como intocável.

**O `dogma.db`.** `banco_do_cliente()` migra `dogma.db` → `seele.db` desde a
0.7.7, levando os três arquivos (`.db`, `-wal`, `-shm`) juntos. Ele **tem** de
continuar procurando o nome velho: aquele arquivo guarda o certificado TLS do
servidor, e abrir um banco novo vazio ao lado de um cheio geraria chave nova e
dispararia o alarme de pino trocado do [ADR 0003](0003-certificados-tofu.md) — o
alarme reservado a ataque — em todo mundo que já entrou naquele servidor.

**O `Line` do `ratatui`.** `ui.rs` e `selecao.rs` importam `ratatui::text::Line`
para renderizar. O `Node::Line` de domínio dentro deles foi renomeado por nome; o
do ratatui ficou.

### O formato de fio

`postcard` serializa variante por **índice**, não por nome, então nenhum dos
renames quebra compatibilidade de protocolo. A exceção é o `seele://`: o
parâmetro `cage=` virou `room=`, e um link antigo cai na regra do
[ADR 0006](0006-esquema-de-uri.md) — parâmetro desconhecido é ignorado, não
recusado — perdendo a sala escolhida em silêncio. Aceitável enquanto o público é
beta, e dito aqui em vez de descoberto depois.

## Consequências

**Fica mais fácil** ler um relatório de bug: `docs/glossario.md` tinha duas
colunas que diziam conceitos diferentes — "sala de voz" na tela e `Cage` no
código — e agora diz a mesma coisa em duas línguas.

**Fica mais difícil** ler o histórico. Um `git log -S Cage` atravessa a fronteira
de 2026-08-25 e para; quem investigar um defeito antigo vai encontrar dois
vocabulários e precisar deste ADR para saber qual é qual. É o custo de preservar
os registros em vez de reescrevê-los, e é o lado certo da troca.

**Um padrão apareceu três vezes e vale como aviso permanente.** Um guarda que
procura o termo aposentado tem a agulha reescrita pela varredura que o aposenta,
e passa a procurar a palavra que existe para proteger. O guarda da tela de
entrada chegou a acusar «SEELE» de ser citação do anime — o nome do próprio
produto. As duas entradas de `APOSENTADOS` e a lista de citações em
`apps/seele-app/tests/frontend.rs` carregam esse aviso escrito.

**E uma classe de defeito não tem compilador nem guarda: texto fora do código.**
O `Info.plist` — a caixa de permissão de microfone do macOS, a primeira coisa que
todo usuário daquele sistema lê — ficou dizendo «transmitir sua voz aos outros
pessoas do VoiceRoom … para além do Dogma ao qual você se conectou». Foi achado
por varredura manual, não por teste.

## Custo de reverter

**Alto, e cresce.** Nove migrações irreversíveis por decisão de `specs/04`, e
todo banco que subir uma vez passa a ter as formas novas. Reverter o código sem
reverter o banco produz consultas contra tabelas que não existem mais; reverter o
banco não é possível sem migrações de volta, que este projeto recusa por escrito.

O momento de discordar era antes das migrações 5 a 9. Depois delas, o caminho de
volta é um backup.

## Adendo — três linhas do mapa só fecharam em 2026-09-05

A tabela acima diz `Plug` · `InsertPlug` · `EjectPlug` → `Connection` ·
`EnterVoiceRoom` · `LeaveVoiceRoom`. **Duas dessas três não tinham acontecido.**
`Connection` sim; os dois verbos não, e sobreviveram onze dias em quatro
camadas — `seele_core::Client`, o `Comando` do `Enlace`, a `Connection` do
`seele-ffi`, e os dois comandos Tauri que o JavaScript chama pelo nome.

O mesmo vale para `CasperCompartilhado`, apelido público de
`Arc<Mutex<Persistence>>` em `hospedagem.rs`, e para o `casper_hospedado` que o
app usava para o alcançar. A varredura dos três Magi passou pelos módulos e não
pelos apelidos deles.

**Por que nenhum guarda pegou, e é a parte que vale registrar.** A varredura
tinha uma agulha em `apps/seele-app/tests/frontend.rs` — a lista `APOSENTADOS`,
que procura palavra aposentada no **texto que se lê na tela**. `insert_plug` é
identificador, não texto: a própria `parece_identificador` o descarta de
propósito, porque a alternativa acusaria todo `invoke("apagar_voice_room")` da
janela. O guarda estava certo e não era este o trabalho dele. **Não havia guarda
nenhum para identificador Rust**, e não há agora — o que existe é `cargo check`,
que só cobra coerência, nunca vocabulário.

O que fechou agora, e como se sabe que fechou:

| era | é | onde |
|---|---|---|
| `Client::insert_plug` · `eject_plug` | `enter_voice_room` · `leave_voice_room` | `seele-core` |
| `Enlace::inserir_plug` · `ejetar_plug` | `entrar_na_voice_room` · `sair_da_voice_room` | `seele-core` |
| `Comando::InserirPlug` · `EjetarPlug` | `EntrarNaVoiceRoom` · `SairDaVoiceRoom` | `seele-core` |
| `Connection::insert_plug` · `eject_plug` | `enter_voice_room` · `leave_voice_room` | `seele-ffi` |
| comandos Tauri `insert_plug` · `eject_plug` | `enter_voice_room` · `leave_voice_room` | app + `ui/` |
| `CasperCompartilhado` · `casper_hospedado` | `PersistenceCompartilhada` · `persistence_hospedada` | `seele-server` + app |

**O par mais arriscado era o comando Tauri**, porque o nome atravessa uma
fronteira que o compilador não lê: o JavaScript o chama por string. Ele já tinha
guarda, e o guarda foi provado por reversão — devolvendo `invoke("insert_plug")`
ao `tela-sessao.js`, `every_command_the_frontend_calls_is_registered` reprova
com «the frontend calls commands that main.rs does not register:
["insert_plug"]», e `no_command_is_registered_and_never_called` reprova do outro
lado. Os dois juntos são o que impede meia renomeação de passar.

### O que **não** foi tocado, e cada motivo é o mesmo de antes

- **As migrações 1 e 9 do `schema.rs`.** A primeira semeia `"InsertPlug"` e a
  nona o troca por `"EnterVoiceRoom"`. São texto entregue, e a regra
  `append only once shipped` não abre exceção para arrumação.
- **A chave de permissão `inserir_plug`.** `specs/04-servidor-seele.md` e o
  `docs/glossario.md` a declaram como identificador que a configuração em disco
  procura — «as chaves **não mudam**».
- **A coluna esquerda de `APOSENTADOS`**, e as citações `«INSERIR PLUG»`,
  `«EJETAR PLUG DO OPERADOR»`, `«PLUG EJETADO»`, `«Entry Plug»` e `5 PLUGS`. São
  as agulhas dos guardas e as legendas dos comps: reescrevê-las é o defeito que
  este ADR já nomeou três vezes.
- **`plug.exe`** no instalador, que é o nome de um arquivo real deixado por uma
  instalação antiga.

### E uma classe de dano que a varredura de agosto deixou para trás

Ao renomear, `plug` virou `connection` **dentro de prosa em português**, e o
resultado eram frases que ninguém consegue ler: «ejetar o connection», «o
connection do Comandante saiu da sala», «a silhueta do connection de entrada» —
esta última descrevendo a marca antiga, onde a palavra certa era e continua
sendo `plug`. Dezessete delas foram consertadas junto com este trabalho, em
`voice_room.rs`, `session.rs`, `conhecidos.rs`, `client.rs`, `control.rs`,
`frases.js`, `camada-moderar.js`, `marca.rs` e quatro testes de conformidade.

**O dano do mesmo gênero foi varrido junto, contado e não estimado.** `Cage` era
masculino e `sala de voz` é feminino; `Pilot` era masculino e `pessoa` é
feminino; `Line` era feminino e `canal` é masculino — e a varredura trocou o
substantivo sem tocar no artigo, no adjetivo nem no particípio.

| classe | corrigidas |
|---|---|
| `o` / `um` / `num` / `último` **sala de voz**, `dois salas de voz` | 30 |
| `o` / `um` / `este` / `aquele` **pessoa**, `dois pessoas` | 41 |
| frase de doc **em inglês** com sintagma português dentro — `Renames o canal`, `Opens o canal and asks for the page of history behind it` | 28 |
| achadas ao alargar a busca: `todos as salas de voz`, `mede a sinal`, `sala de voz inteiro`, `sala de voz vazio`, `sala de voz antigo`, `pessoa banido` | 8 |

Nenhuma delas chegava a texto de tela — as duas dentro de `ui/` eram JSDoc —, o
que é o guarda `APOSENTADOS` tendo feito exatamente o trabalho dele e nenhum
outro.

**Foram feitas uma a uma, e o motivo é este ADR.** Uma segunda varredura por
expressão regular sobre `\bo sala de voz\b` acertaria o artigo e deixaria
«a segunda sala de voz não foi **feito**» e «entrar numa sala de voz
**trancado**» de pé — que é a mesma classe de erro, um passo adiante. As
contagens acima se refazem assim:

```sh
grep -rnoE "\b(o|no|do|ao|um|num|pelo|esse|este|aquele|outro|mesmo|último|novo|nenhum|todo|primeiro|segundo|terceiro) sala de voz\b" --include="*.rs" --include="*.js" crates apps
grep -rnoE "\b(o|no|do|ao|um|num|pelo|esse|este|aquele|outro|mesmo|último|nenhum|todo) pessoa\b" --include="*.rs" --include="*.js" crates apps
grep -rnoE "(sala de voz|salas de voz|pessoa|pessoas) (trancado|cheio|vazio|aberto|inteiro|sozinho|banido|antigo|novo)s?\b" --include="*.rs" --include="*.js" crates apps
```

As três voltam vazias hoje. **A coluna esquerda de `APOSENTADOS` continua
intocada**, e é por isso que a primeira delas não pode virar guarda de CI sem
antes ganhar a mesma isenção que aquela lista carrega.
