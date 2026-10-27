#!/bin/sh
# Regrava `pasta/` com o código da v0.15.1, sem tocar no repositório de onde é
# chamado (a travessia API 5 → API 6; ver o LEIA.md ao lado). O mesmo roteiro
# do `../pasta-da-0.15.1/gerar.sh`, com o pacote `perfis-3.3.1/`.
#
#   sh gerar.sh <pasta-de-trabalho>
set -eu
V0151=54f542f94
AQUI=$(cd "$(dirname "$0")" && pwd)
REPO=$(git -C "$AQUI" rev-parse --show-toplevel)
mkdir -p "$1"
PASTA=$(cd "$1" && pwd)
rm -rf "$PASTA/fonte" "$PASTA/saida"
mkdir -p "$PASTA/fonte"

git -C "$REPO" archive "$V0151" | tar -x -C "$PASTA/fonte"
cp "$AQUI/gerar.rs" "$PASTA/fonte/crates/seele-server/examples/gerar-pasta-perfis.rs"
( cd "$PASTA/fonte" \
  && CARGO_TARGET_DIR="$PASTA/alvo" cargo run -q -p seele-server --example gerar-pasta-perfis \
       -- "$PASTA/saida" "$AQUI/perfis-3.3.1" )

rm -rf "$AQUI/pasta"
mkdir -p "$AQUI/pasta"
cp -R "$PASTA/saida/mod-packages" "$AQUI/pasta/"
sqlite3 "$PASTA/saida/seele.db" .dump > "$AQUI/pasta/seele.sql"
echo "pronto: $AQUI/pasta"
