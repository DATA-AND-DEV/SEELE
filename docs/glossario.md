# Glossário normativo — pt-BR · en

**Autoridade sobre a palavra que aparece na tela.** Desde o **ADR 0033**, o
vocabulário da interface é o comum — servidor, sala de voz, canal de texto,
pessoa — e não mais o de `specs/07-tema-evangelion.md`. A `07` continua no
repositório com uma nota no alto dizendo o que foi retirado; o que ela ainda
governa é a estética, não a língua.

Duas colunas e um corte no meio:

- **na tela** é o que a pessoa lê. Muda por este documento.
- **identificador** é o nome real da coisa no código. **Não muda.** `Cage`
  continua `Cage`, `Dogma` continua `Dogma`, `at_field` continua `at_field`.
  Renomear tipo é outro trabalho, com outro custo, e o ADR 0033 decidiu
  explicitamente não fazê-lo.

Ler as duas colunas juntas é o ponto do documento: quem lê um relatório de bug
que diz «não consigo entrar na sala 2» precisa achar `CageId` sem intermediário.

Regra que sobrevive à `07` e vale aqui inteira: **o nome nunca custa clareza.**
Ela agora custa menos, porque o nome comum já é a explicação.

## Glossário canônico

| Conceito | na tela (pt-BR) | na tela (en) | Identificador Rust |
|---|---|---|---|
| Instância de servidor | servidor | server | `Dogma` |
| Daemon | `seeled` | `seeled` | `seeled` |
| Cliente de terminal | `plug` | `plug` | `plug` |
| Canal de voz | sala de voz | voice room | `Cage`, `CageId` |
| Coluna de salas | SALAS DE VOZ | VOICE ROOMS | — |
| Sala sem nome | SALA 1, SALA 2 | ROOM 1, ROOM 2 | — |
| Canal de texto | canal de texto | text channel | `Line`, `LineId` |
| Coluna de canais | CANAIS | CHANNELS | — |
| Usuário | pessoa | person | `Pilot`, `PilotId` |
| Nome escolhido pela pessoa | APELIDO | NICKNAME | `Pilot::nick` |
| Entrar em sala de voz | conectar · CONECTAR | connect · CONNECT | `insert_plug` |
| Sair da sala de voz | sair · SAIR · sair da sala | leave · LEAVE | `eject` |
| Mudo (microfone) | mudo · microfone fechado | muted · mic closed | `at_field` |
| Qualidade de conexão | sinal · SINAL | signal · SIGNAL | `SyncRatio` |
| Faixas do sinal | boa · fraca · crítica | good · weak · critical | ver ADR 0024 |
| Sessão verificada | conexão segura · CONEXÃO SEGURA | secure connection | `Pattern::Blue` |
| Latência | atraso | delay | `signal_delay_ms` |
| Perda de pacote | perda | loss | `HarmonicDisturbance`, `loss_pct` |

**`Entry Plug` fica**, e só como nome interno: é o nome do cliente de terminal
no código, na documentação de arquitetura e na forma desenhada da marca. Não é
rótulo de tela. O binário continua `plug`.

## Papéis

| na tela (pt-BR) | na tela (en) | Identificador |
|---|---|---|
| Comandante | Commander | `Role::Commander` |
| Operador | Operator | `Role::Operator` |
| Piloto | Pilot | `Role::Pilot` |
| Observador | Observer | `Role::Observer` |

**A colisão antiga acabou sozinha.** «O Piloto não tem permissão» era ambíguo
porque `Piloto` nomeava ao mesmo tempo a conta e o papel. A conta agora é
**pessoa**; `Piloto` sobrou só como papel, e a frase deixa de ter dois sentidos.

**Ponto em aberto:** `Role::Pilot` é o único rótulo de papel que ainda vem da
`07`. Os outros três são português comum e nada os obriga a mudar. O mapa de
renomeação não decide este caso, e este documento não o inventa — fica como
está até que decidam.

## Subsistemas

Fronteiras reais de módulo. **Não mudam, e o ADR 0033 diz isso em voz alta:**

| Nome | Responsabilidade | Identificador |
|---|---|---|
| PERMISSIONS | Identidade, autenticação, sessões, papéis, permissões | `Subsystem::Permissions` |
| MEDIA | Roteamento de mídia, encaminhamento, controle de banda | `Subsystem::Media` |
| PERSISTENCE | Estado persistente, histórico, configuração, migrações | `Subsystem::Persistence` |

O que saiu foi o **diagrama das três luzes no rodapé**, não os três subsistemas.
As luzes nunca mediram nada — eram cenário se passando por instrumento, e um
instrumento falso é consultado justamente quando algo dá errado. A telemetria
que **informa** ficou inteira, e é ela que aparece na tela agora.

## Mensagens de protocolo

`specs/02-protocolo.md` nomeia as mensagens em português. No código elas são:

| `02` (pt) | Identificador | Direção |
|---|---|---|
| `Ola` | `Hello` | cliente → servidor |
| `Resposta` | `Response` | cliente → servidor |
| `InserirPlug` | `InsertPlug` | cliente → servidor |
| `EjetarPlug` | `EjectPlug` | cliente → servidor |
| `EntrarNaLinha` | `JoinLine` | cliente → servidor |
| `EnviarMensagem` | `SendMessage` | cliente → servidor |
| `BuscarHistorico` | `FetchHistory` | cliente → servidor |
| `DefinirATField` | `SetAtField` | cliente → servidor |
| `DefinirEstado` | `SetPresence` | cliente → servidor |
| `Ping` | `Ping` | cliente → servidor |
| `Desafio` | `Challenge` | servidor → cliente |
| `Sessao` | `Session` | servidor → cliente |
| `UsuarioEntrou` | `PilotJoined` | servidor → cliente |
| `UsuarioSaiu` | `PilotLeft` | servidor → cliente |
| `EstadoUsuario` | `PilotState` | servidor → cliente |
| `MensagemRecebida` | `MessageReceived` | servidor → cliente |
| `MensagemEditada` | `MessageEdited` | servidor → cliente |
| `MensagemRemovida` | `MessageRemoved` | servidor → cliente |
| `Telemetria` | `Telemetry` | servidor → cliente |
| `Alerta` | `Alert` | servidor → cliente |
| `Pong` | `Pong` | servidor → cliente |
| `Desconectando` | `Disconnecting` | servidor → cliente |

Estes são nomes de mensagem: nenhum deles aparece na tela, e por isso o ADR 0033
não os toca. A divergência que o glossário antigo apontava — `02` diz
`UsuarioEntrou` e o código diz `PilotJoined` — deixou de ser deriva de
vocabulário e virou o que sempre foi por baixo: uma tradução pt↔en de nome de
identificador, coberta pelo ADR 0023.

## Motivos de erro

Todos enumerados — `02` proíbe string livre chegando à interface.

| na tela (pt-BR) | Identificador |
|---|---|
| conexão segura não estabelecida | `PatternBlueNotEstablished` |
| versão incompatível | `Incompatible` |
| manutenção programada | `ScheduledMaintenance` |

A forma canônica do identificador continua `PatternBlue`, não `BluePattern`. O
motivo original — «o identificador espelha a exibição» — caiu com a exibição;
o identificador fica pelo motivo de sempre, que é não renomear tipo de graça.

## Permissões

`specs/04-servidor-seele.md`, modelo enumerado, sem sistema de expressão.
As chaves **não mudam**: são identificadores que a configuração em disco procura.

| na tela (pt-BR) | Chave · identificador |
|---|---|
| ver a sala | `ver_cage` · `Permission::ViewCage` |
| conectar na sala | `inserir_plug` · `Permission::InsertPlug` |
| falar | `falar` · `Permission::Speak` |
| ler o canal | `ler_linha` · `Permission::ReadLine` |
| escrever no canal | `escrever_linha` · `Permission::WriteLine` |
| remover mensagem | `remover_mensagem` · `Permission::RemoveMessage` |
| mover pessoa | `mover_piloto` · `Permission::MovePilot` |
| expulsar | `expulsar` · `Permission::Kick` |
| banir | `banir` · `Permission::Ban` |
| gerenciar salas | `gerenciar_cages` · `Permission::ManageCages` |
| gerenciar papéis | `gerenciar_papeis` · `Permission::ManageRoles` |
| administrar o servidor | `administrar_dogma` · `Permission::AdministerDogma` |

## Japonês

**Fora da interface.** O katakana e o kanji decorativo — `第3新東京市`, `同期率`,
`警告` — saíram da tela pelo ADR 0033. Não foram traduzidos: foram removidos,
porque nunca carregaram informação necessária para operar o produto.

`ゼーレ` continua, e só como **marca**: assinatura, ícone, cartela, inicialização,
README. As regras estão em `docs/marca.md`, e nenhuma delas é vocabulário de
tela.

Onde ainda houver kanji num contexto de terminal, a regra de largura continua de
pé: `unicode-width`, nunca `.len()` — `05` avisa que isso quebra o layout.

## Ainda sem entrada no mapa

Termos da `07` que aparecem na tela, que este documento **não** decide porque o
mapa de renomeação não os cobre. Ficam registrados para quem fechar o mapa:

| na tela hoje | conceito | sugestão, não normativa |
|---|---|---|
| Isolamento total | surdo (alto-falante) | «surdo» / «som desligado» |
| Distúrbio harmônico | perda de pacote | «perda» |
| Bateria interna | reconectando | «reconectando» |
| Terminal Dogma | configurações | «configurações» |
| Alerta · 警告 | notificação crítica | «alerta», sem o kanji |

Aplicar qualquer uma delas exige entrada no mapa. Até lá, quem encontrar uma
destas strings **deixa como está** — é mais barato deixar uma para trás do que
inventar um nome que a próxima tela não vai repetir.

## O que este glossário não decide

A postura de direitos sobre a franquia (`07`, `[EM ABERTO]`). A recomendação do
plano continua sendo repositório privado até M4. O ADR 0033 encolheu a
superfície do problema sem fechá-lo: `A.T. Field` saiu da tela, e `Entry Plug`
sobrou como nome interno e como forma da marca desenhada. `PERMISSIONS`,
`MEDIA` e `PERSISTENCE` são nomes bíblicos e seguros. O risco que resta está no
repositório, não no produto.
