#!/bin/sh
# M3 da 0.16 — a carga-cartoes (a lista de 251 cartões de MOD e o teto
# agregado de 128), na pasta de uma campanha da API 6 já preparada, **sem
# recompilar o app**: os cenários rodam na mesma build dos outros, para serem
# lidos contra o mesmo `sem-mod`.
#
#   sh preparar-cartoes.sh <pasta-da-campanha>
#
# Nesta ordem:
#   1. confere que o roteiro da pasta é o da campanha de 10/10 (`0f1303bc`), e
#      o troca pelo da evidência, que tem o cenário; se a cópia de trabalho foi
#      mexida, para, para ninguém perder o que mexeu;
#   2. o falante com `--calados`: copia o `m3-falante.rs`, aplica o remendo do
#      número da voz (`remendar-api6.py`, item 4) e recompila **só o exemplo**;
#   3. o `ax` com a fundura opcional no `textos`;
#   4. gera a variante `cartoes` numa pasta à parte, confere-a contra o
#      prelúdio da cópia (`conferir-carga`), e põe só ela nos pacotes e no cache
#      por hash. As outras variantes ficam como foram medidas.
set -eu
PASTA=$(cd "$1" && pwd)
AQUI=$(cd "$(dirname "$0")" && pwd)
REPO=$(git -C "$AQUI" rev-parse --show-toplevel)
ROTEIRO_DA_CAMPANHA=0f1303bc
PORTA_DA_CONFERENCIA=8392  # a 8391 é da campanha

git -C "$REPO" show "$ROTEIRO_DA_CAMPANHA:docs/evidencias/m3-custo-dos-mods/m3.py" \
  | cmp -s - "$PASTA/roteiro/m3.py" \
  || { echo "o roteiro de $PASTA não é o da campanha ($ROTEIRO_DA_CAMPANHA): junte à mão"; exit 1; }
cp "$AQUI/m3.py" "$PASTA/roteiro/m3.py"

FALANTE="$PASTA/fonte/crates/seele-core/examples/m3-falante.rs"
cp "$AQUI/m3-falante.rs" "$FALANTE"
python3 -I - "$FALANTE" <<'PY'
import sys
caminho = sys.argv[1]
texto = open(caminho).read()
antes = "version: seele_proto::PROTOCOL_VERSION,"
assert texto.count(antes) == 1, "o remendo do número da voz não pegou"
open(caminho, "w").write(texto.replace(antes, "version: seele_proto::MEDIA_HEADER_VERSION,"))
PY
( cd "$PASTA/fonte" && CARGO_TARGET_DIR="$PASTA/alvo" cargo build --release -p seele-core --example m3-falante )
swiftc -O "$AQUI/ax.swift" -o "$PASTA/ax"

OBRAS=$(mktemp -d)
python3 -I "$AQUI/gerar-carga.py" "$OBRAS" > /dev/null
VARIANTE="$PASTA/pacotes/m3-carga-cartoes"
rm -rf "$VARIANTE"
cp -R "$OBRAS/m3-carga-cartoes" "$VARIANTE"
rm -rf "$OBRAS"
"$PASTA/alvo/release/examples/conferir-carga" "$VARIANTE/cliente/main.js" \
  || { echo "a variante cartoes tropeçou no prelúdio da cópia"; exit 1; }

# O hash pelo próprio servidor da medição, como no `preparar-api6.sh`.
rm -rf "$PASTA/conferencia" "$PASTA/conferencia.txt"
"$PASTA/alvo/release/examples/m3-servidor" "$PORTA_DA_CONFERENCIA" "$PASTA/conferencia" "$VARIANTE" \
  2>/dev/null > "$PASTA/conferencia.txt" &
SERVIDOR=$!
for _ in 1 2 3 4 5 6 7 8 9 10; do [ -s "$PASTA/conferencia.txt" ] && break; sleep 1; done
kill "$SERVIDOR" 2>/dev/null || true
wait "$SERVIDOR" 2>/dev/null || true
HASH=$(sed -n 's/.*#\([0-9a-f]\{64\}\).*/\1/p' "$PASTA/conferencia.txt")
[ -n "$HASH" ] || { echo "sem hash para a variante cartoes"; exit 1; }
rm -rf "$PASTA/conferencia" "$PASTA/conferencia.txt" "$PASTA/cache/mod-packages/$HASH"
cp -R "$VARIANTE" "$PASTA/cache/mod-packages/$HASH"
echo "m3-carga-cartoes: conferida, no cache como $HASH"
echo "pronto: sh $PASTA/roteiro/campanha-api6.sh carga-cartoes carga-cartoes+voz"
