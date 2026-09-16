#!/bin/zsh
# Condensa um registro bruto de `cargo test` preservando, verbatim: o cabeçalho
# de carga, cada linha `Running`/`Doc-tests`, cada `test result:`, cada
# reprovação (com a mensagem do pânico), o bloco `failures:` e o rodapé com o
# tempo de parede. O que sai são as linhas `test ... ok`, que são o volume.
set -u
IN="$1"; OUT="$2"
{
  head -1 "$IN"
  echo
  awk '
    /^     Running|^   Doc-tests|^test result:|^error: test failed/ { print; next }
    / panicked at /            { print; guarda = 1; next }
    guarda == 1                { print; guarda = 0; next }
    /\.\.\. FAILED$/           { print; next }
    /^failures:$/              { if (!vistas[FNR-1]++) print; next }
    /^    [a-z_0-9:]+$/        { print; next }
  ' "$IN"
  echo
  tail -1 "$IN"
} > "$OUT"
