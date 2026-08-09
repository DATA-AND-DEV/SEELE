# Glossário normativo — pt-BR · en

`specs/10-convencoes.md` declara o glossário de `specs/07-tema-evangelion.md`
normativo **nos dois idiomas**, mas `07` só existe em português e `10` dá apenas
três exemplos. Este documento fecha a lacuna.

**É pré-requisito de qualquer nome de tipo, módulo ou variante de erro.** Código,
identificadores e comentários são em inglês (`10`), então a coluna EN não é
tradução de cortesia: é o nome real das coisas no código.

Regra que vem de `07` e vale aqui: **o tema nunca custa clareza.** Nomeação
temática vem sempre acompanhada do dado concreto — `Distúrbio harmônico · perda
8,4%`, nunca `Distúrbio harmônico` sozinho.

## Glossário canônico

| Conceito | pt-BR | en | Identificador Rust |
|---|---|---|---|
| Instância de servidor | Dogma Central | Central Dogma | `Dogma` |
| Daemon | `seeled` | `seeled` | `seeled` |
| Cliente | Entry Plug · `plug` | Entry Plug · `plug` | `plug` |
| Canal de voz | Cage | Cage | `Cage`, `CageId` |
| Canal de texto | Linha | Line | `Line`, `LineId` |
| Usuário | Piloto | Pilot | `Pilot`, `PilotId` |
| Entrar em canal de voz | Inserir plug | Insert plug | `insert_plug` |
| Sair do canal de voz | Ejetar | Eject | `eject` |
| Qualidade de conexão | Taxa de Sincronização | Sync Ratio | `SyncRatio` |
| Latência | Atraso de sinal | Signal Delay | `signal_delay_ms` |
| Perda de pacote | Distúrbio harmônico | Harmonic Disturbance | `HarmonicDisturbance`, `loss_pct` |
| Mudo (microfone) | A.T. Field ativo | A.T. Field engaged | `at_field` |
| Surdo (alto-falante) | Isolamento total | Total Isolation | `total_isolation` |
| Sessão verificada | PADRÃO: AZUL | PATTERN: BLUE | `Pattern::Blue` |
| Sessão não verificada | PADRÃO: LARANJA | PATTERN: ORANGE | `Pattern::Orange` |
| Reconectando | Bateria interna | Internal Battery | `InternalBattery` |
| Notificação crítica | Alerta · 警告 | Alert · 警告 | `Alert` |
| Configurações | Terminal Dogma | Terminal Dogma | `TerminalDogma` |

## Papéis

| pt-BR | en | Identificador |
|---|---|---|
| Comandante | Commander | `Role::Commander` |
| Operador | Operator | `Role::Operator` |
| Piloto | Pilot | `Role::Pilot` |
| Observador | Observer | `Role::Observer` |

**Colisão conhecida:** `Piloto` é ao mesmo tempo o conceito de conta de usuário
(`specs/04`, modelo de domínio) e o nome de um papel. Em Rust os dois vivem em
namespaces distintos — `Pilot` e `Role::Pilot` — e isso resolve. Em texto de
interface, não: escrever "o Piloto não tem permissão" é ambíguo. Preferir "a
conta" ou "o papel Piloto" conforme o caso.

## Subsistemas

| Nome | Responsabilidade | Identificador |
|---|---|---|
| MELCHIOR | Identidade, autenticação, sessões, papéis, permissões | `Subsystem::Melchior` |
| BALTHASAR | Roteamento de mídia, encaminhamento, controle de banda | `Subsystem::Balthasar` |
| CASPER | Estado persistente, histórico, configuração, migrações | `Subsystem::Casper` |

Estado nominal: "os três concordam". Não é decoração — são fronteiras reais de
módulo e o estado de cada um aparece no diagnóstico do cliente (`specs/04`).

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

**Correção de vocabulário:** `02` usa `UsuarioEntrou` / `UsuarioSaiu` /
`EstadoUsuario`, mas `07` define que o usuário se chama **Piloto**. É deriva de
vocabulário exatamente do tipo que `07` manda evitar. O código usa `Pilot*`;
`specs/02-protocolo.md` deveria ser corrigida.

## Motivos de erro

Todos enumerados — `02` proíbe string livre chegando à interface.

| pt-BR | Identificador |
|---|---|
| `PadraoAzulNaoEstabelecido` | `PatternBlueNotEstablished` |
| `Incompatível` | `Incompatible` |
| `ManutencaoProgramada` | `ScheduledMaintenance` |

A forma canônica é `PatternBlue`, não `BluePattern` — a exibição é
`PADRÃO: AZUL` / `PATTERN: BLUE`, e o identificador espelha a exibição.

## Permissões

`specs/04-servidor-seele.md`, modelo enumerado, sem sistema de expressão.

| pt-BR | Identificador |
|---|---|
| `ver_cage` | `Permission::ViewCage` |
| `inserir_plug` | `Permission::InsertPlug` |
| `falar` | `Permission::Speak` |
| `ler_linha` | `Permission::ReadLine` |
| `escrever_linha` | `Permission::WriteLine` |
| `remover_mensagem` | `Permission::RemoveMessage` |
| `mover_piloto` | `Permission::MovePilot` |
| `expulsar` | `Permission::Kick` |
| `banir` | `Permission::Ban` |
| `gerenciar_cages` | `Permission::ManageCages` |
| `gerenciar_papeis` | `Permission::ManageRoles` |
| `administrar_dogma` | `Permission::AdministerDogma` |

## Kanji

Acento tipográfico, **sempre secundário**. Nunca carrega informação necessária
para operar o produto; quem não lê japonês não perde nada (`07`).

Fragmentos aprovados: 警告 (alerta) · 同期率 (taxa de sincronização) ·
第3新東京市 · 発令.

Na TUI, kanji ocupa duas células. Largura sempre por `unicode-width`, nunca por
`.len()` — `05` avisa que isso quebra o layout se for esquecido.

## O que este glossário não decide

A postura de direitos sobre a franquia (`07`, `[EM ABERTO]`). A recomendação do
plano é repositório privado até M4. Quando a decisão vier, note que a exposição
não é uniforme: `MELCHIOR`/`BALTHASAR`/`CASPER` são nomes bíblicos e seguros;
`Cage`, `Piloto`, `Linha`, `Dogma Central` são vocabulário genérico; o risco se
concentra em **`A.T. Field`** e **`Entry Plug`**, que são cunhagens da obra.
