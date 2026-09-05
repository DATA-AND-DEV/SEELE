# 0008 — `shiguredo_opus` como binding do codec

Status: aceito · substitui a primeira redação deste ADR

> **Vocabulário.** Esta página é anterior ao [ADR
> 0035](0035-o-codigo-deixa-de-falar-evangelion.md) e diz `Cage` onde o
> produto hoje diz **sala de voz**. O texto fica como foi escrito: o 0035
> preserva de propósito o registro de ontem, e o `docs/glossario.md` é a
> autoridade sobre a palavra de hoje.

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

## Revisão em M5 — o pré-compilado não serve, e passamos a compilar do fonte

Ao gerar instaladores para os três sistemas, o caminho pré-compilado do crate falhou em **duas das três plataformas**, e a inspeção mostrou que a terceira também tem limite sério. Os três problemas foram verificados, não deduzidos:

1. **Windows.** O `.tar.gz` publicado para `windows_x86_64` contém `lib/opus.lib`; o `build.rs` copia `lib/libopus.a` com o nome fixo no código. A compilação morre **depois** de baixar e conferir o checksum com sucesso, o que torna a mensagem de erro especialmente enganosa. Baixei o arquivo publicado e listei o conteúdo.
2. **macOS Intel não existe.** O mapa de alvos cobre `macos_arm64` e nada mais para Apple. Um `.dmg` universal — o único que serve, já que metade dos Macs em uso ainda é Intel — é impossível por esse caminho.
3. **Linux só Ubuntu.** `detect_linux_distro()` entra em pânico fora de Ubuntu 22.04, 24.04 e 26.04. Num produto cujo argumento central é ser auto-hospedado, um servidor que não compila em Debian, Fedora ou Alpine não é uma limitação aceitável.

Decisão: ligar a feature `source-build` **em todas as plataformas**, compilando o libopus do código-fonte.

Não é preferência por compilar do fonte; é parar de depender de uma matriz de binários que não cobre o nosso caso. O `shiguredo_cmake` continua baixando o próprio CMake, então nenhum pré-requisito de sistema aparece — o argumento original do ADR segue valendo.

Custo medido: cerca de meio minuto por build limpo, uma vez por plataforma, com cache no CI. O ganho é as três plataformas se comportarem igual, o que num projeto com matriz de três SOs vale bem mais que trinta segundos.

Verificado localmente antes de confiar: compila em `aarch64-apple-darwin` e em `x86_64-apple-darwin`, os oito testes do codec passam com o libopus construído do fonte, e o binário universal sai com as duas arquiteturas.

**O que isto diz sobre a escolha do crate.** É o segundo defeito de terceiro que o Opus nos custa, depois do `audiopus` em M0.4. O `shiguredo_opus` continua sendo a melhor opção da mesa — é mantido, tem `decode_plc` e `decode_fec` de primeira classe, e o benchmark acima segue válido. Mas o suporte a plataforma dele claramente não é exercitado fora de Linux/Ubuntu e macOS ARM, que devem ser os alvos de quem o publica. Com `source-build` ligado, essa parte do crate deixa de estar no nosso caminho. **Se aparecer um terceiro defeito, a escolha merece ser reaberta** — e o candidato seria escrever o wrapper direto sobre `libopus` com `bindgen`, que é o que este crate faz.

Custo de reverter: **baixo**. A superfície usada é pequena e está isolada em `seele-audio`.
