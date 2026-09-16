#!/bin/zsh
# Variante de `condensa.sh` para os registros do `rodada4.sh`, cujo cabeçalho de
# carga tem DUAS linhas (a carga em si e a contagem de processos do usuário) em
# vez de uma. O corte do corpo é idêntico, linha por linha, ao de `condensa.sh`:
# só muda quantas linhas de cabeçalho são preservadas verbatim.
set -u
IN="$1"; OUT="$2"
{
  head -2 "$IN"
  echo
  awk '
    /^     Running|^   Doc-tests|^test result:|^error: test failed/ { print; next }
    /^Error: /                 { print; next }
    / panicked at /            { print; guarda = 1; next }
    guarda == 1                { print; guarda = 0; next }
    /\.\.\. FAILED$/           { print; next }
    /^failures:$/              { if (!vistas[FNR-1]++) print; next }
    /^    [a-z_0-9:]+$/        { print; next }
  ' "$IN"
  echo
  tail -1 "$IN"
} > "$OUT"
