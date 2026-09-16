#!/bin/zsh
set -u
for r in 1 2 3 4 5; do
  while pgrep -x cargo >/dev/null 2>&1; do sleep 5; done
  QUEIMADORES=8 /tmp/o29/rodada4.sh "verde8_${r}" cargo test --workspace
done
echo "FIM DAS 5 RODADAS VERDES"
