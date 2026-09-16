#!/bin/zsh
# $1 = prefixo, $2 = max rodadas, $3.. = comando
# Espera pelo BINÁRIO cargo (pgrep -x), e não por qualquer linha de comando que
# contenha "cargo test": as sessões vizinhas deixam shells de espera cuja linha
# contém exatamente esse texto, e `pgrep -f` casava com elas para sempre.
set -u
PREFIXO="$1"; MAX="$2"; shift 2
for r in $(seq 1 $MAX); do
  while pgrep -x cargo >/dev/null 2>&1; do sleep 5; done
  /tmp/o29/rodada3.sh "${PREFIXO}_${r}" "$@"
  SAIDA=$(grep -o 'SAIDA=[0-9]*' /tmp/o29/${PREFIXO}_${r}.log | tail -1 | cut -d= -f2)
  if [[ "$SAIDA" != "0" ]]; then echo "REPROVOU na rodada $r"; exit 1; fi
done
echo "TODAS AS $MAX RODADAS SAIRAM 0"
