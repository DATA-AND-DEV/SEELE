# API 8 no Windows — decisões (o Forge e o NeoForge, e a árvore do processo)

Sobre `desenho/api-8-sobre-0.19` a partir de `58a2a3c9`, com o MOD de
multiplayer em `desenho-0.19` a partir de `d8c8c90`. O dono: o Windows é o
sistema de quem joga, e o MOD tem de hospedar modpacks Forge e NeoForge nele.
Dois buracos: a oferta tinha um `rodar` (e um `instalar`) para todo sistema, e
o Forge e o NeoForge rodam por `@libraries/…/unix_args.txt` (classpath com
`:`), quando o Windows pede o `win_args.txt` (com `;`); e parar matava só o
filho no Windows (a decisão R8 deixou o I9 aberto lá).

Conferido em 31/10/2026 pelo `curl` do CLAUDE.md: a mais nova é a v0.18.3
(publicada às 04:30Z de 30/10), e a v8 (protocolo 12) segue sem release — a
`api/v8.json` ainda emenda, e o campo novo da oferta entra no 12 pela regra de
acréscimo (ADR 0058) e pelo precedente do `plataformas` e do `instalar`.

Cada decisão no formato `Ruling: <o quê> — <por quê> — <custo se errada>`.

## O que o instalador real escreve (medido neste Mac, 31/10/2026)

Os dois instaladores de verdade, rodados num Mac ARM com o Java da Adoptium
(Forge 1.20.1-47.4.28 com o 17.0.21+9; NeoForge 21.1.265 com o 21.0.13+9; o
hash de cada instalador conferido com o `.sha256` do Maven), código 0 nos dois:

- **Os dois escrevem os dois arquivos, em qualquer sistema**:
  `libraries/net/minecraftforge/forge/1.20.1-47.4.28/{unix,win}_args.txt`
  (5224 bytes cada) e `libraries/net/neoforged/neoforge/21.1.265/{unix,win}_args.txt`
  (6119 bytes cada), mais `run.bat`, `run.sh` e `user_jvm_args.txt`. O
  `run.bat` é `java @user_jvm_args.txt @libraries/…/win_args.txt %*`.
- **A forma**: 16 linhas (Forge) e 18 (NeoForge), LF, sem CR, sem `\`, sem
  caminho absoluto; cada linha é um argumento ou um par «opção valor». O
  `win_args.txt` é o `unix_args.txt` com `;` no lugar de `:` — byte a byte
  (`win.replace(';', ':') == unix`), e as duas linhas que mudam são o `-p`
  (o module path) e o `-DlegacyClassPath=`.
- **Todo caminho que o `win_args.txt` cita existe depois da instalação**: 58
  (Forge) e 66 (NeoForge) jars sob `libraries/`, todos com `/`. (O
  `-DignoreList=` cita nomes de arquivo, e não caminhos.)
- As cópias de verdade viraram fixtures do MOD
  (`test/fixtures/{forge,neoforge}-{win,unix}_args.txt`).

## Decisões

- **Ruling W1: a oferta carrega o par de comandos por sistema, em
  `por_sistema`, e o par de uma entrada substitui o de cima inteiro.**
  `OfertaDeMaquina.por_sistema: Vec<ComandosDoSistema>` (no fim da struct,
  `#[serde(default)]`), cada entrada com `plataformas` (de
  `PLATAFORMAS_DE_MAQUINA`, ao menos uma, cada sistema numa entrada só, até
  cinco entradas) e `rodar`/`instalar` opcionais. `comandos_para(oferta,
  plataforma)` é a regra única: a entrada que nomeia o sistema dá o par; sem
  entrada, vale o de cima. A política (`politica_do_aceite`) monta `comando` e
  `instalar` por ela, então o R1 prende o `@…/win_args.txt` e o
  `java/bin/java.exe` no Windows e o `unix_args.txt` no Mac, com a mesma
  comparação sem caixa e pelas duas barras (teste
  `a_politica_aceita_e_prende_o_par_deste_sistema`, que confere
  `LIBRARIES\net\MinecraftForge\…\WIN_ARGS.TXT`). O resumo amarra todas as
  entradas (a oferta inteira entra nele). — Por que um par inteiro, e não
  `plataformas` em cada `ComandoOferecido`: com marca em cada comando, um
  sistema podia ficar com dois `rodar` ou com o `instalar` de um e o `rodar`
  de outro, e a regra de qual vale seria uma ordem; a entrada inteira diz o
  que roda ali sem combinação. E uma entrada sem `instalar` é «sem passo de
  instalar ali», e não herda o de cima: a tela mostra exatamente o que roda.
  — Custo: (a) um MOD que só quer trocar o `rodar` no Windows repete o
  `instalar` na entrada (o MOD de referência repete); (b) campo novo no 12 —
  o ouro do fio foi regerado só na linha do `PedidoDeMaquina` e a semente do
  fuzz da oferta, e um aceite guardado antes (sem a chave) lê `por_sistema`
  vazio, o mesmo resumo de antes.
- **Ruling W2: a tela do aceite mostra só o par desta máquina, e diz quantos
  outros sistemas têm o seu.** `PedidoNaTela` ganha `rodar_daqui`,
  `instalar_daqui` e `sistemas_com_outros_comandos`, calculados por
  `seele_ffi::programa::comandos_desta_maquina` (a mesma `comandos_para`), e a
  casca desenha esses, com a linha «outro sistema tem comandos próprios, que
  não rodam aqui». `por_sistema` entra na lista de «o que mudou» («os
  comandos de cada sistema»). Bancada `aceite-da-maquina.cjs`, caso 18, nos
  dois motores (19 casos, 0 falhas). — Por que dizer a contagem: o aceite
  vale a oferta inteira (o resumo), e uma pessoa que aceita no Mac e depois
  abre no Windows não vê pedido de novo; a linha avisa que há outro par. —
  Custo: a tela não mostra o par dos outros sistemas; quem quiser lê-lo
  precisa abrir o pedido na outra máquina (ou ler o MOD).
- **Ruling W3: no Windows, o filho de `rodar` entra num Job Object com
  `JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE`, e parar mata a tarefa.**
  `programa/tarefa_windows.rs`: `CreateJobObjectW` anônima,
  `SetInformationJobObject` com o limite, `AssignProcessToJobObject` com a
  alça do `tokio::process::Child`; o `Grupo` da R8 guarda a `Tarefa`, e
  `Grupo::matar` (DESLIGAR, REVOGAR, app fechando, `encerrar_tudo`,
  `Programa` solto, e o filho que sai sozinho) chama `TerminateJobObject`; o
  `Drop` fecha a alça (e o `KILL_ON_JOB_CLOSE` mata o que restar). Se o
  Windows não puser o filho na tarefa, o `rodar` segue e o log diz `WARN`
  «parar mata só ele, e não os netos». O teste dos netos
  (`parar_mata_os_netos_por_todos_os_caminhos`) saiu de trás do
  `#[cfg(unix)]` e roda nos dois sistemas (o programa de teste sobe o neto
  pela `std`); o `o_filho_entra_na_tarefa_e_morre_com_ela` é
  `#[cfg(windows)]` (confere `IsProcessInJob`). — Por que agora: o dono pôs o
  Windows como o sistema de quem joga, e o I9 inteiro era lá (um `java.exe`
  órfão segura a porta e o mundo). E o Job Object fecha também o custo (b) da
  R8 no Windows: o app que cai fecha as alças ao morrer, e o sistema mata a
  tarefa. — Custo: (a) a tarefa não liga `JOB_OBJECT_LIMIT_BREAKAWAY_OK`, então um
  processo que o jogo tente criar fora dela (`CREATE_BREAKAWAY_FROM_JOB`) não
  escapa — o `CreateProcess` dele falha; um jogo que dependa disso quebra (o
  servidor de Minecraft não depende); (b) o
  app rodando dentro de uma tarefa que proíbe tarefas aninhadas (Windows 7)
  cai no `WARN` e mata só o filho.
- **Ruling W4: o filho entra na tarefa logo depois de nascer, e não
  suspenso.** O `std` não dá a alça da thread principal para o
  `ResumeThread` de um `CREATE_SUSPENDED`, e o atributo `JOB_LIST` do
  `STARTUPINFOEX` (`raw_attribute`) segue instável no Rust 1.97. Retomar pela
  `Toolhelp32` ou pelo `NtResumeProcess` seria mais `unsafe` e um modo de
  falha novo (um processo que fica suspenso). — Custo: um neto criado entre o
  `CreateProcess` e o `AssignProcessToJobObject` (microssegundos) nasce fora
  da tarefa. O filho é o `java.exe`, que leva centenas de milissegundos
  carregando a JVM antes de criar qualquer processo.
- **Ruling W5: `seele-core` troca a folha do workspace por uma cópia com
  `unsafe_code = "deny"`, e o `allow` mora num módulo só.** O `forbid` não
  se derruba com `allow`; a régua é a do `seele-video` (ADR 0041): `deny` no
  crate, `#![allow(unsafe_code)]` só em `programa/tarefa_windows.rs`
  (compilado só no Windows), cada bloco `unsafe` com o seu `SAFETY:`. O resto
  da folha é copiado do workspace. **Zero crate novo no `Cargo.lock`**: o
  `windows` 0.62 já vinha pelo `seele-audio` e pelo `seele-video`; entram uma
  aresta de `seele-core` e as features `Win32_System_JobObjects`,
  `Win32_Security` e `Win32_System_Threading`. `cargo xtask check-deps`
  passa (a regra é de crates do workspace). — Custo: quem mexer na folha do
  workspace precisa lembrar das três cópias (`seele-video`, `seele-audio`,
  `seele-core`).
- **Ruling W6 (MOD): o par do Windows do Forge e do NeoForge nomeia
  `java/bin/java.exe`, e o Vanilla e o Fabric seguem com `java/bin/java`.**
  Ver `DECISOES.md` do MOD (W-mod-1). — Custo: dois jeitos de escrever o
  mesmo Java numa receita.

## O que não roda neste Mac

- `cargo check --target x86_64-pc-windows-msvc -p seele-core` **não
  compila daqui**: o `ring` (e o `openh264`, e o `opus`) compilam C para o
  alvo, e o `cc` do Mac não tem os cabeçalhos do MSVC (`'assert.h' file not
  found`). O código do Windows foi compilado e medido no PC Windows (abaixo).
- O `o_filho_entra_na_tarefa_e_morre_com_ela` é `#[cfg(windows)]`: no Mac
  ele não existe, e não conta como prova aqui.
