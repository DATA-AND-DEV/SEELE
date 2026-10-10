#!/bin/sh
# A campanha da M3 com a API 6 (tarefa 11 do plano da F3), na build de medição
# de `preparar-api6.sh`. Cada cenário espera a máquina sem compilação e ociosa
# (o m3.py sai com 3 ou 4 enquanto espera); um cenário interrompido por
# entrada, ou sujo por uma compilação de outra sessão, é repetido uma vez; uma
# falha de outro tipo é registrada e a campanha segue para o próximo.
#
# Sem argumentos, os 24; com argumentos, só os cenários pedidos.
cd "$(dirname "$0")/.." || exit 1
mkdir -p medidas
APP="$(pwd)/alvo/release/bundle/macos/SEELE.app"
[ -d "$APP" ] || { echo "sem a build de medição em $APP: rode o preparar-api6.sh"; exit 1; }
# Step 1: os dez da base, na build da 0.16 com a 6 aceita.
BASE="sem-mod 1-mod 3-mods 3-mods-editor 3-mods-mesa sem-mod+voz 1-mod+voz 3-mods+voz 3-mods-editor+voz 3-mods-mesa+voz"
# Step 2: os novos — o arraste sem pausa, e o MOD de carga em cada ponto no teto.
NOVOS="3-mods-mesa-continuo 3-mods-mesa-continuo+voz carga-regioes carga-regioes+voz carga-capas carga-capas+voz carga-folhas carga-avisos carga-avisos+voz carga-palco carga-dialogo+voz carga-pagina+voz carga-tudo carga-tudo+voz"
CENARIOS=${*:-$BASE $NOVOS}
for c in $CENARIOS; do
  for tentativa in 1 2; do
    while :; do
      python3 -I roteiro/m3.py "$c" --app "$APP" --segundos 60 --ocioso 300 > "medidas/ultimo-$c.json" 2> "medidas/ultimo-$c.err"
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
