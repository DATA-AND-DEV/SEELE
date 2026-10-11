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
#   3. copia o falante da evidência (o do «diz e segue», `94fee0c0`) e aplica os
#      remendos (`remendar-conserto-63.py`): o `tela-sessao.js` do `6717dc2c`,
#      letra por letra, e o número da voz;
#   4. recompila o app e o falante na mesma pasta de alvo;
#   5. **marca a cópia**: o `COMMIT_DA_MEDICAO` passa a `<commit>+6717dc2c`, e
#      cada relato o leva. O `resumir-api6.py` recusa misturar commits, e é isso
#      que impede ler a `carga-cartoes` consertada contra o `sem-mod` de antes.
set -eu
PASTA=$(cd "$1" && pwd)
AQUI=$(cd "$(dirname "$0")" && pwd)
COMMIT_DA_CAMPANHA=6e845e0d
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

cp "$AQUI/m3-falante.rs" "$PASTA/fonte/crates/seele-core/examples/m3-falante.rs"
python3 -I "$AQUI/remendar-conserto-63.py" "$PASTA/fonte"

( cd "$PASTA/fonte/apps/seele-app" \
  && CARGO_TARGET_DIR="$PASTA/alvo" cargo tauri build --bundles app )
( cd "$PASTA/fonte" \
  && CARGO_TARGET_DIR="$PASTA/alvo" cargo build --release -p seele-core --example m3-falante )

echo "$COMMIT+$CONSERTO" > "$PASTA/fonte/COMMIT_DA_MEDICAO"
echo "pronto: o app e o falante com o conserto, marcados $COMMIT+$CONSERTO"
echo "        o da campanha ficou em alvo/release/bundle/macos/SEELE-$COMMIT_DA_CAMPANHA.app"
# As referências também: o `resumir-api6.py` lê cada cenário contra o `sem-mod`
# (ou o `sem-mod+voz`) do mesmo commit, e o app mudou.
echo "        sh $PASTA/roteiro/campanha-api6.sh sem-mod sem-mod+voz carga-cartoes carga-cartoes+voz"
