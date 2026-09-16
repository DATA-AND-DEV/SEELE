#!/bin/zsh
# $1 = rótulo do log, $2..= comando cargo
set -u
export PATH="$HOME/.cargo/bin:$PATH"
cd "/Users/dev-alexandre/Documents/Obsidian Vault/Órbita/.orbita/worktrees/299c2412-1abd-4516-9748-226ce1950e3e"
ROTULO="$1"; shift
LOG="/tmp/o29/${ROTULO}.log"

# --- carga concorrente deliberada: 12 queimadores de CPU numa máquina de 15 ---
PIDS=()
for i in $(seq 1 12); do
  ( while :; do :; done ) & PIDS+=($!)
done
sleep 3
echo "carga ligada: ${#PIDS[@]} processos; load antes = $(uptime | sed 's/.*averages: //')" > "$LOG"

INI=$(date +%s.%N)
"$@" >> "$LOG" 2>&1
SAIDA=$?
FIM=$(date +%s.%N)

kill ${PIDS[@]} 2>/dev/null
wait 2>/dev/null

PAREDE=$(echo "$FIM - $INI" | bc)
echo "=== ROTULO=$ROTULO SAIDA=$SAIDA PAREDE=${PAREDE}s ===" >> "$LOG"
echo "ROTULO=$ROTULO SAIDA=$SAIDA PAREDE=${PAREDE}s"
