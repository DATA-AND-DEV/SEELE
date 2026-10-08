#!/bin/sh
# A campanha da M3 de base: os dez cenários, em série. Cada um espera a máquina
# sem compilação e ociosa (o m3.py sai com 3 ou 4 enquanto espera); um cenário
# interrompido por entrada, ou sujo por uma compilação de outra sessão, é
# repetido uma vez; uma falha de outro tipo é
# registrada e a campanha segue para o próximo.
cd "$(dirname "$0")/.." || exit 1
mkdir -p medidas
for c in sem-mod 1-mod 3-mods 3-mods-editor 3-mods-mesa sem-mod+voz 1-mod+voz 3-mods+voz 3-mods-editor+voz 3-mods-mesa+voz; do
  for tentativa in 1 2; do
    while :; do
      python3 -I roteiro/m3.py "$c" --segundos 60 --ocioso 300 > "medidas/ultimo-$c.json" 2> "medidas/ultimo-$c.err"
      codigo=$?
      [ "$codigo" -eq 3 ] || [ "$codigo" -eq 4 ] || break
      echo "$(date +%H:%M:%S) $c: esperando (saída $codigo)"; sleep 60
    done
    if [ "$codigo" -ne 0 ]; then echo "$(date +%H:%M:%S) $c: FALHOU (saída $codigo)"; break; fi
    tail -1 "medidas/ultimo-$c.json" | grep -q '"interrompido": false' \
      && tail -1 "medidas/ultimo-$c.json" | grep -q '"rustc_durante": 0' && break
    echo "$(date +%H:%M:%S) $c: interrompido por entrada ou sujo por compilação, repetindo"
  done
  echo "$(date +%H:%M:%S) $c: fim"
done
