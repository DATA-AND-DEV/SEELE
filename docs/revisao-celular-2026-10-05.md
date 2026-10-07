# Revisão do app no celular — 05/10/2026

Branch `mobile/ios`. A pergunta era se o app de iOS já se deixa testar: instalar,
entrar, hospedar, navegar, falar, sair — sem ficar preso em tela nenhuma. A
resposta veio de duas medidas, e não de olhar:

- **`tools/auditoria-celular.py`** abre cada tela e camada da casca a 390x844
  com `plataforma = ios` e mede saída alcançável (teste de `elementFromPoint` no
  centro do controle), página rolando de lado, alvos abaixo de 44px, títulos
  sobrepostos e erros de script. São 34 estados: a entrada, hospedar, a sessão,
  as duas gavetas, a busca, a chamada, as dez seções das configurações e as
  treze camadas.
- **O simulador do iOS 27** (iPhone 18 Pro Max) com o app compilado, o log do
  sistema e o `seele.log` do contêiner do app.

## O que travava o uso

| # | Achado | Causa | Conserto |
|---|---|---|---|
| 1 | Na chamada não havia como voltar à conversa nem sair da sala. | CANAIS e PESSOAS moravam na barra da conversa, que some na chamada. O SAIR DA SALA está no operador, dentro da gaveta de canais. | No celular as duas portas vão para uma faixa própria entre a trilha e o conteúdo (`levarAsPortas`, `tela-sessao.js`), visível na conversa e na chamada. |
| 2 | A gaveta de pessoas não fechava; a de canais só fechava escolhendo algo. | O primeiro degrau de celular as abria por cima da tela inteira — inclusive da porta que as fecharia. | As gavetas abrem na célula do conteúdo e nunca cobrem a faixa das portas (`celular.css`). |
| 3 | Configurações com uma palavra por linha nas dez seções. | Duas colunas (216px de menu + corpo) mantidas em 390px. | Duas etapas: a lista inteira; a seção inteira com «‹ SEÇÕES» (`server-secoes`, `data-vista`). |
| 4 | Autenticação rolando de lado, 820px. | Três colunas com mínimos de 300 + 220 + 300px. | Uma coluna, a entrada primeiro. O VOLTAR À ENTRADA, escondido no aperto, aparece no celular. |
| 5 | Fim de sessão rolando de lado, 481px. | O cartão `.boot` tem 480px fixos. | Largura da tela. |
| 6 | O app caía no arranque. | O SDK do iOS 27 exige o ciclo de vida por cenas; o `tao` 0.35 só o liga com `UIApplicationSupportsMultipleScenes` verdadeiro. | `UIApplicationSceneManifest` no `Info.ios.plist`. |
| 7 | O microfone não abria («Invalid property value»). | A sessão de áudio do iOS nasce em `SoloAmbient`, que não grava. | `gen/apple/Sources/seele-app/sessao-de-audio.m`: `PlayAndRecord` com alto-falante e Bluetooth, ativada no arranque, e a licença do microfone pedida ali. |
| 8 | Com o microfone em TECLA — o padrão —, ele nunca abriria no celular. | Push-to-talk é segurar uma tecla. | No celular, sem escolha gravada, o modo é VOZ (`lib.rs`, ao conectar); TECLA, ATALHOS e o bloco das teclas da ajuda saem da tela pela plataforma. |

## Segunda rodada, no simulador

| Achado | Causa | Conserto |
|---|---|---|
| Na chamada, não se achava como sair da sala. | O SAIR DA SALA morava no pé da gaveta de canais. | No celular a tira do operador vira o rodapé fixo da sessão — microfone, fone, configurações, SAIR DA SALA, COMPARTILHAR — na conversa e na chamada (`levarAsPortas`). |
| PESSOAS e depois CANAIS não mudava a tela. | As duas gavetas abriam independentes, e a de canais nascia por baixo da de pessoas. | Abrir uma fecha a outra, como abas. |
| Sair da sala deixava a tela numa grade vazia. | O comentário prometia voltar aos canais, mas o botão próprio dele tinha sido fundido no `operador-sair`, e a volta não veio junto. | `SAIR DA SALA` fecha a chamada. |
| De volta à conversa, o rodapé ainda dizia SAIR DA SALA. | Só a chamada redesenhava o rótulo, e só aberta. | O rodapé é redesenhado a cada quadro da sessão. |

Cada um virou uma prova em `tools/auditoria-celular.py`, e cada prova foi vista
falhar com o conserto desligado antes de passar com ele.

**Um tropeço de processo, registrado para não repetir.** O `cargo tauri ios
build` termina dizendo BUILD SUCCEEDED e, no último passo, falha ao copiar o
`.app` para `gen/apple/build/arm64-sim/` quando a pasta já existe — e o `.app`
antigo fica lá. Um teste inteiro no simulador rodou a casca de vinte minutos
antes. `tools/ios-simulador.sh` limpa a pasta, compila e recusa instalar um
`.app` que não seja o desta compilação.

## O que atrapalhava

| Achado | Conserto |
|---|---|
| A página desenhada a 980px e encolhida. | `<meta name="viewport">`, com guarda em `tests/frontend.rs`. |
| Títulos sobrepostos ao quebrar (PREPARAR ESTE SERVIDOR, ONDE VOCÊ JÁ ESTEVE, CONEXÃO PERDIDA, CONEXÃO SEGURA). | O `body` fixa `line-height: 16px` e todo título herdava; no celular os títulos ganham 1,12. |
| Alvos de toque de 16 a 34px em quase toda tela. | 44px de altura em botões, campos e resumos, com `:not(#_)` para vencer as regras de classe sem `!important`; 44px de largura nos botões de um glifo. Sobram as caixas de marcar, cujo toque é o rótulo. |
| O volume da chamada vazava sobre o cartão de baixo. | `grid-auto-rows: max-content` — com `auto` a grade usava a contribuição mínima do cartão (medido: 160px contra 200px). |
| O nome do servidor guardado quebrando letra a letra. | O nome com linha própria; os botões descem. |
| Ajuda e Compartilhar fechando num botão escrito «ESC». | `data-rotulo-celular="FECHAR"` e `aria-label` — que faltava também no desktop. |
| A ajuda em duas colunas espremidas. | Termo e definição empilhados. |
| O ícone padrão do Tauri. | `design/marca/gerar-icones.py` gera o conjunto do iOS: quadrado, sangrado, sem alfa. |
| Instalar MOD por pasta: o celular não tem seletor de pasta. | Recusa nomeada `SemSeletorDePasta`, com frase; o catálogo continua instalando. |

## Para compilar no Xcode 27

| Falha | Conserto |
|---|---|
| Versão mínima 14.0; o Xcode 27 aceita a partir de 15.0. | `deploymentTarget: 16.0` em `gen/apple/project.yml` — `:has()` e `dvh` pedem 15.4. O `bundle > iOS > minimumSystemVersion` do `tauri.conf.json` não chega ao projeto no tauri-cli 2.11.4 (medido). |
| O Swift do Tauri compilado contra o SDK do macOS. | `swift-rs` 1.0.8, que passa `--triple` ao SwiftPM do Xcode 27. |
| `rquickjs-sys` sem bindings para iOS. | Feature `bindgen` só no alvo iOS. |
| Link sem os símbolos do libopus. | `shiguredo_opus` 2026.3.0, que trata o iOS como Mach-O. |
| `pick_folder` e `set_fullscreen` só existem no desktop. | `cfg(desktop)`/`cfg(mobile)`. |

## O que ficou aberto

- **Áudio no simulador — resolvido fora do código.** Com a sessão configurada,
  a unidade de áudio do iOS esperava o servidor de áudio e estourava o tempo
  (`Initialize: RPC timeout. Apparently deadlocked`). Trocar o modo `VoiceChat`
  pelo padrão não mudou nada (medido). A causa era a licença de microfone do
  **macOS** para o app Simulator: ligada, a sala de voz funcionou de ponta a
  ponta — supressão de ruído tratando o sinal, ganho automático assentando em
  1,67×, o cartão em FALANDO, e entrar, ver a chamada, voltar e sair da sala
  sem ficar preso. Quem for testar no simulador precisa da mesma licença.
- **Compartilhar a tela do celular.** O iOS só deixa gravar a tela por uma
  extensão de transmissão (ReplayKit), um alvo nativo à parte do app, com a
  transmissão chegando ao núcleo em Rust. Hoje a camada abre e diz que a lista
  fica vazia; receber a tela de outra pessoa funciona.
- **Tela bloqueada e chamada telefônica** (atividade ao vivo, CallKit): os
  concepts de 04/10 desenham; nada disso existe ainda.
- **Aparelho de verdade**: exige o Team ID da Apple (`bundle > iOS >
  developmentTeam`), e a conta é de quem publica.
- Um `tauri ios init` novo reescreve `gen/apple/project.yml` (volta a 14.0) e o
  `main.mm`; o `sessao-de-audio.m` e o `Info.ios.plist` sobrevivem.
