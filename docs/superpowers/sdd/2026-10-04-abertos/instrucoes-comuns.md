# Instruções comuns — os abertos da 1.0 (execução por subagentes)

Diretório de trabalho: /Users/dev-alexandre/SEELE/.claude/worktrees/abertos-da-1.0 (git worktree, branch
conserto/abertos-da-1.0, a partir do main local 3685a64). Rode todo comando daqui; nunca faça cd para
/Users/dev-alexandre/SEELE nem mexa em outro worktree. Nunca use `git stash` puro. Ninguém empurra nada (push, tag,
release, Actions): o commit local é o fim.

## Restrições

- Português nos comentários, nas frases de tela e nas mensagens de asserção. Cada mensagem diz O QUE QUEBRA, e toda
  asserção nova leva mensagem.
- O fio não muda: `PROTOCOL_VERSION` continua 8, o SEELE-ENC/1 fica igual, e um cliente 0.15.0 e este branch
  continuam conversando. A API de MODs não muda: `api/v3..v5.json` ficam intocados, e `cargo xtask check-api` e o
  guarda de congelamento continuam verdes. A CSP não afrouxa.
- Nenhuma dependência nova, e nada baixado da rede.
- `unwrap`/`expect` só em teste, e nada de `x[i]` nem `s[a..b]` em `src/`. `missing_docs` e `unreachable_pub` avisam,
  e o clippy roda com `-D warnings`.
- Código com `cfg(windows)` ou `cfg(target_os = "linux")` não compila neste Mac: escreva-o com o mesmo cuidado, diga
  no relatório o que não pôde ser compilado nem rodado aqui, e prefira lógica pura (sem `cfg`) testável em qualquer
  plataforma.
- Testes do implementador: pontuais (o crate ou o binário que você mexeu). Teste de conformidade toma
  `let _vaga = vaga::minha();` na primeira linha e precisa da porta UDP 8384 livre. A bateria inteira roda depois,
  num passo próprio do fluxo do lote.
- Todo guarda é provado por reversão: desfazer o conserto, ver vermelho com a frase dele, restaurar e conferir byte a
  byte.
- Antes de cada commit: `cargo fmt --all -- --check` (depois `cargo fmt --all`) e
  `cargo clippy --workspace --all-targets --all-features -- -D warnings`. O aviso do linker do macOS
  (`ld: duplicate -rpath`) é ambiental.
- Bancadas Playwright, sem baixar nada:
  `PLAYWRIGHT=/Users/dev-alexandre/SEELE-MOD-PERFIS/node_modules/playwright perl -e 'alarm 240; exec @ARGV' node …`.
  Todo comando longo roda com `perl -e 'alarm N; exec @ARGV' …`, porque o macOS não tem `timeout`.
- A mensagem de commit é uma frase em português que diz o que mudou, terminando com a linha Co-Authored-By do SEU
  ambiente. Nenhum assunto contém uma das catorze cadeias do G1 (a lista está no plano 1B, «O portão G1»).

## Processo
- A lista de cada lote está em `lotes.md` (neste diretório), na seção do lote. O inventário verificado que a
  originou está em `inventario.json`. O ledger é `progress.md`.
- Se algo contradisser o código, NEEDS_CONTEXT, sem adivinhar. TDD quando houver comportamento. Você não dispara
  subagentes.
- O relatório vai no arquivo que o despacho indicar, com RED/GREEN, as reversões, os arquivos mudados e as
  preocupações.
