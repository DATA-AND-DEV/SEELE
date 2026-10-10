#!/bin/sh
# Regrava `pasta/` com o código da v0.15.1, sem tocar no repositório de onde é
# chamado (tarefa 18 da F3; ver o LEIA.md ao lado).
#
#   sh gerar.sh <pasta-de-trabalho>
#
# 1. copia o commit da v0.15.1 por `git archive` para <pasta>/fonte;
# 2. põe ali o `gerar.rs` como exemplo do `seele-server` e o roda, com o
#    pacote `estilo-3.1.0/`, escrevendo em <pasta>/saida;
# 3. troca o `pasta/` daqui pela saída, com o banco em texto (`seele.sql`, o
#    `.dump` do `sqlite3`) no lugar do binário.
set -eu
V0151=54f542f94
AQUI=$(cd "$(dirname "$0")" && pwd)
REPO=$(git -C "$AQUI" rev-parse --show-toplevel)
mkdir -p "$1"
PASTA=$(cd "$1" && pwd)
rm -rf "$PASTA/fonte" "$PASTA/saida"
mkdir -p "$PASTA/fonte"

git -C "$REPO" archive "$V0151" | tar -x -C "$PASTA/fonte"
cp "$AQUI/gerar.rs" "$PASTA/fonte/crates/seele-server/examples/gerar-pasta-da-0-15.rs"
( cd "$PASTA/fonte" \
  && CARGO_TARGET_DIR="$PASTA/alvo" cargo run -q -p seele-server --example gerar-pasta-da-0-15 \
       -- "$PASTA/saida" "$AQUI/estilo-3.1.0" )

rm -rf "$AQUI/pasta"
mkdir -p "$AQUI/pasta"
cp -R "$PASTA/saida/mod-packages" "$AQUI/pasta/"
sqlite3 "$PASTA/saida/seele.db" .dump > "$AQUI/pasta/seele.sql"
echo "pronto: $AQUI/pasta"
