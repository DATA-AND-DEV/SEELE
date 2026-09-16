#!/bin/zsh
# Carga concorrente REAL: além dos queimadores de CPU, N processos rodando
# binários de conformidade já compilados, fora do `cargo` (portanto fora da
# trava do diretório de compilação, e fora da vaga, que é por processo).
# É a carga histórica da pendência 29: vários servidores QUIC de verdade.
set -u
export PATH="$HOME/.cargo/bin:$PATH"
W="/Users/dev-alexandre/Documents/Obsidian Vault/Órbita/.orbita/worktrees/299c2412-1abd-4516-9748-226ce1950e3e"
cd "$W"
ROTULO="$1"; shift
LOG="/tmp/o29/${ROTULO}.log"
BINS=(target/debug/deps/acceptance_m2-* target/debug/deps/salas-* target/debug/deps/bateria_interna-*)
BINS=(${(@)BINS:#*.d})

PIDS=()
for i in $(seq 1 ${QUEIMADORES:-8}); do ( while :; do :; done ) & PIDS+=($!); done
for b in $BINS; do ( while :; do "$b" >/dev/null 2>&1; done ) & PIDS+=($!); done
sleep 5
echo "carga: ${QUEIMADORES:-8} queimadores + ${#BINS} laços de conformidade fora do cargo; load = $(uptime | sed 's/.*averages: //')" > "$LOG"

INI=$(date +%s.%N)
"$@" >> "$LOG" 2>&1
SAIDA=$?
FIM=$(date +%s.%N)
kill ${PIDS[@]} 2>/dev/null; pkill -f 'target/debug/deps/acceptance_m2-' 2>/dev/null
pkill -f 'target/debug/deps/salas-' 2>/dev/null; pkill -f 'target/debug/deps/bateria_interna-' 2>/dev/null
wait 2>/dev/null
PAREDE=$(echo "$FIM - $INI" | bc)
echo "=== ROTULO=$ROTULO SAIDA=$SAIDA PAREDE=${PAREDE}s ===" >> "$LOG"
echo "ROTULO=$ROTULO SAIDA=$SAIDA PAREDE=${PAREDE}s"
