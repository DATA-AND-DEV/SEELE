#!/bin/zsh
# Carga concorrente deliberada que RESPEITA o alcance do conserto: apenas
# queimadores de processador disputando a máquina com a bateria.
#
# POR QUE ESTE ARQUIVO EXISTE, ao lado de `rodada4.sh`: aquele acrescenta laços
# que rodam binários de conformidade JÁ COMPILADOS fora do `cargo` — cada um
# levanta servidores QUIC de verdade num processo que não é o da bateria. A
# permissão da fila é um semáforo de processo: ela não governa, nem poderia
# governar, processos de fora. Aquela carga, portanto, reintroduz exatamente a
# contenção que o conserto remove, e serve para medir o limite do conserto, não
# para medir se ele funciona. Esta aqui satura a máquina sem burlá-lo.
set -u
export PATH="$HOME/.cargo/bin:$PATH"
W="/Users/dev-alexandre/Documents/Obsidian Vault/Órbita/.orbita/worktrees/299c2412-1abd-4516-9748-226ce1950e3e"
cd "$W"
ROTULO="$1"; shift
LOG="/tmp/o29/${ROTULO}.log"

PIDS=()
for i in $(seq 1 ${QUEIMADORES:-8}); do ( while :; do :; done ) & PIDS+=($!); done
sleep 5
echo "carga: ${QUEIMADORES:-8} queimadores de processador, nenhum binário de conformidade fora do cargo; load = $(uptime | sed 's/.*averages: //')" > "$LOG"
echo "processos do usuario com a carga ligada: $(ps -u $(id -u) | wc -l | tr -d ' ')" >> "$LOG"

INI=$(date +%s.%N)
"$@" >> "$LOG" 2>&1
SAIDA=$?
FIM=$(date +%s.%N)
kill ${PIDS[@]} 2>/dev/null
wait 2>/dev/null
PAREDE=$(echo "$FIM - $INI" | bc)
echo "=== ROTULO=$ROTULO SAIDA=$SAIDA PAREDE=${PAREDE}s ===" >> "$LOG"
echo "ROTULO=$ROTULO SAIDA=$SAIDA PAREDE=${PAREDE}s"
