#!/bin/sh
# M3 da 0.16 — a `carga-cartoes` de novo, depois do conserto da pendência 63,
# na pasta da campanha da API 6 (`preparar-api6.sh` e `preparar-cartoes.sh` já
# rodados nela). Decisão da orquestradora, 10/10: com 251 pessoas, na build da
# M3 remendada com o conserto, para saber se o teto de 256 alvos se sustenta
# com o produto consertado.
#
#   sh preparar-conserto-63.sh <pasta-da-campanha>
#
# Nesta ordem:
#   1. confere que a cópia é a da campanha (`COMMIT_DA_MEDICAO` do `6e845e0d`)
#      e que ainda não foi marcada (uma preparação que parou no meio roda de
#      novo);
#   2. guarda o app da campanha ao lado (`SEELE-6e845e0d.app`), uma vez só: os
#      24 cenários foram medidos nele;
#   3. troca o roteiro da pasta pelo da evidência, com a `lista-251` (a
#      referência da `carga-cartoes`: os mesmos calados, sem MOD); se o da pasta
#      não é o que o `preparar-cartoes.sh` pôs (`8fe32c96`), para, para ninguém
#      perder o que mexeu;
#   4. copia o falante da evidência (o do «diz e segue», `94fee0c0`) e aplica os
#      remendos (`remendar-conserto-63.py`): o `tela-sessao.js` do `6717dc2c`,
#      letra por letra, e o número da voz;
#   5. recompila o app e o falante na mesma pasta de alvo;
#   6. **marca a cópia**: o `COMMIT_DA_MEDICAO` passa a `<commit>+6717dc2c`, e
#      cada relato o leva. O `resumir-api6.py` recusa misturar commits, e é isso
#      que impede ler a `carga-cartoes` consertada contra o `sem-mod` de antes.
set -eu
PASTA=$(cd "$1" && pwd)
AQUI=$(cd "$(dirname "$0")" && pwd)
REPO=$(git -C "$AQUI" rev-parse --show-toplevel)
COMMIT_DA_CAMPANHA=6e845e0d
ROTEIRO_DOS_CARTOES=8fe32c96
CONSERTO=6717dc2c
APP="$PASTA/alvo/release/bundle/macos/SEELE.app"

COMMIT=$(cat "$PASTA/fonte/COMMIT_DA_MEDICAO")
case "$COMMIT" in
  *+*) echo "a cópia já foi remendada ($COMMIT)"; exit 1 ;;
  "$COMMIT_DA_CAMPANHA"*) ;;
  *) echo "a cópia de $PASTA é do $COMMIT, e não da campanha ($COMMIT_DA_CAMPANHA): junte à mão"; exit 1 ;;
esac
[ -d "$APP" ] || { echo "sem o app da campanha em $APP"; exit 1; }

# Só na primeira vez: numa preparação que parou no meio, o app daqui pode ser
# um empacotamento pela metade, e o guardado é o da campanha.
GUARDADO="$PASTA/alvo/release/bundle/macos/SEELE-$COMMIT_DA_CAMPANHA.app"
[ -d "$GUARDADO" ] || ditto "$APP" "$GUARDADO"

# O roteiro: o que o `preparar-cartoes.sh` pôs, ou já o desta evidência (uma
# preparação que parou no meio).
if ! cmp -s "$AQUI/m3.py" "$PASTA/roteiro/m3.py"; then
  git -C "$REPO" show "$ROTEIRO_DOS_CARTOES:docs/evidencias/m3-custo-dos-mods/m3.py" \
    | cmp -s - "$PASTA/roteiro/m3.py" \
    || { echo "o roteiro de $PASTA não é o dos cartões ($ROTEIRO_DOS_CARTOES): junte à mão"; exit 1; }
  cp "$AQUI/m3.py" "$PASTA/roteiro/m3.py"
fi

cp "$AQUI/m3-falante.rs" "$PASTA/fonte/crates/seele-core/examples/m3-falante.rs"
python3 -I "$AQUI/remendar-conserto-63.py" "$PASTA/fonte"

( cd "$PASTA/fonte/apps/seele-app" \
  && CARGO_TARGET_DIR="$PASTA/alvo" cargo tauri build --bundles app )
( cd "$PASTA/fonte" \
  && CARGO_TARGET_DIR="$PASTA/alvo" cargo build --release -p seele-core --example m3-falante )

echo "$COMMIT+$CONSERTO" > "$PASTA/fonte/COMMIT_DA_MEDICAO"
echo "pronto: o app e o falante com o conserto, marcados $COMMIT+$CONSERTO"
echo "        o da campanha ficou em alvo/release/bundle/macos/SEELE-$COMMIT_DA_CAMPANHA.app"
# As referências também, na mesma build: o `resumir-api6.py` lê a
# `carga-cartoes` contra a `lista-251`, a `lista-251` contra o `sem-mod`, e
# recusa misturar commits.
echo "        sh $PASTA/roteiro/campanha-api6.sh sem-mod sem-mod+voz lista-251 lista-251+voz carga-cartoes carga-cartoes+voz"
