# 0008 — `shiguredo_opus` como binding do codec

Status: aceito · substitui a primeira redação deste ADR
Contexto: `specs/03-audio.md` cita "`audiopus` ou `opus`" sem escolher. A primeira redação deste ADR recomendou `audiopus` com libopus vendorizado. **Ao executar M0.4, essa recomendação se mostrou inviável** e foi o retorno mais claro da decisão de puxar a tarefa de M1 para M0.

O que M0.4 encontrou, em ordem:

1. `audiopus 0.2.0` (estável) → `audiopus_sys 0.1.8`, que compila o libopus por **autotools**. Falha aqui por falta de `autoreconf`; no Windows exigiria MSYS2.
2. `audiopus 0.3.0-rc.0` → `audiopus_sys 0.2.2`, que usa **cmake**. Melhor, mas o libopus vendorizado declara `cmake_minimum_required(VERSION 3.1)` e o CMake 4.x removeu compatibilidade abaixo de 3.5. Só compila com o escape hatch `CMAKE_POLICY_VERSION_MINIMUM=3.5`.
3. `cargo deny`, configurado na mesma rodada, reprovou o build: **`audiopus_sys` está formalmente sem manutenção — RUSTSEC-2026-0150**, com "No safe upgrade is available". O advisory descreve exatamente o problema de CMake 4.0 do item 2. Último commit há cinco anos; duas tentativas de contato sem resposta.

Decisão: `shiguredo_opus` (Apache-2.0, versionamento de calendário, `2026.2.0`), da Shiguredo — que desenvolve o SFU WebRTC Sora, ou seja, o mesmo domínio deste projeto.
Alternativas:

- **`audiopus` com o advisory ignorado.** API superior — encode e decode escrevem em buffers do chamador, sem alocar. Descartado: depender de crate abandonado, em pre-release, atrás de um escape hatch que o CMake vai remover, é três riscos empilhados na fundação do milestone mais arriscado.
- **Wrapper próprio sobre um `-sys` mantido.** Daria zero alocação e dependência viva, ao custo de ~200 linhas de `unsafe` nossas e ~3 pontos a mais em M1. Guardado como plano B se a alocação por quadro se mostrar cara na medição.

Consequências:

- Mais fácil: compila **sem workaround nenhum** nos três SOs — o crate vendoriza o próprio cmake (`shiguredo_cmake`), então nem cmake de sistema é pré-requisito. `.cargo/config.toml` não precisa mais de `CMAKE_POLICY_VERSION_MINIMUM`. `decode_plc()` e `decode_fec()` são métodos de primeira classe, exatamente o que o jitter buffer de `03` chama.
- Mais difícil: `encode`/`decode`/`decode_plc` retornam `Vec`, ou seja **alocam por quadro**. Isso não viola `specs/03-audio.md` — a regra de zero alocação vale para o callback do `cpal`, e o codec roda na thread de processamento. Mas são 2 alocações × 50 quadros/s × N fontes, e `specs/10-convencoes.md` diz que regressão no caminho de áudio é bug.

  **Medido em M0.10, e o plano B está aposentado.** `cargo bench --package seele-audio --bench opus_frame`, Apple Silicon, perfil release:

  | | por quadro | % dos 20 ms de tempo real |
  |---|---|---|
  | `encode` | 43,8 µs | 0,22 % |
  | `decode` | 10,5 µs | 0,05 % |
  | `decode_plc` | 10,1 µs | 0,05 % |

  Um cliente em um Cage de 15 pessoas faz 1 encode e 15 decodes por quadro: ≈ 201 µs contra um orçamento de 20 000 µs, ou **1 % de um núcleo**. A alocação não aparece. Escrever wrapper próprio para economizar isso seria otimização sem medida — exatamente o que `specs/10-convencoes.md` proíbe. O benchmark fica como linha de base para detectar regressão.
- **Dívida registrada:** não existe `set_bitrate()`, só `get_bitrate()` — o bitrate é fixado na construção. O bitrate adaptativo de `03` vai precisar de encoder reconstruído ou de patch upstream. Não bloqueia M1; ADR 0010 já adia a política de adaptação para M2. **Reavaliar em M2.**

Medição obtida de brinde em `M0.4`, e que nenhuma spec registra: **lookahead algorítmico do encoder = 312 amostras = 6,50 ms**. Entra no orçamento do ADR 0009.

Custo de reverter: **baixo**. A superfície usada é pequena e está isolada em `seele-audio`.
