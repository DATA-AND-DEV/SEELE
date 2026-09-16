#!/bin/zsh
# Carga concorrente REAL: queimadores de processador + N processos rodando
# binários de conformidade já compilados, fora do `cargo` (portanto fora da
# trava do diretório de compilação, e fora da vaga, que é por processo).
#
# POR QUE ESTE ARQUIVO SUBSTITUI `rodada3.sh`: lá a seleção de binários era
# `target/debug/deps/acceptance_m2-*` com `.d` removido — e isso casa também com
# os milhares de `*.rcgu.o` (unidades de codegen) que a compilação incremental
# deixa no mesmo diretório. Nenhum deles é executável: cada um "roda" em 2 ms
# saindo 126, e o laço que os reinicia vira bomba de forks. Medido: 2.438
# processos, `fork failed: resource temporarily unavailable`, e a máquina sob
# uma carga que NÃO é a de servidores QUIC concorrentes que a §29 pede. A
# seleção agora exige arquivo executável e sem ponto no nome, e a contagem de
# laços é conferida antes de medir.
set -u
export PATH="$HOME/.cargo/bin:$PATH"
W="/Users/dev-alexandre/Documents/Obsidian Vault/Órbita/.orbita/worktrees/299c2412-1abd-4516-9748-226ce1950e3e"
cd "$W"
ROTULO="$1"; shift
LOG="/tmp/o29/${ROTULO}.log"

BINS=()
for p in acceptance_m2 salas bateria_interna; do
  for c in target/debug/deps/${p}-*(N); do
    [[ "${c:t}" == *.* ]] && continue      # descarta .d, .rcgu.o, .dSYM etc.
    [[ -x "$c" && -f "$c" ]] || continue
    BINS+=("$c")
  done
done
if (( ${#BINS} < 3 )); then
  echo "ABORTADO: só ${#BINS} binários de carga encontrados; esperados 3" | tee "$LOG"
  exit 2
fi

PIDS=()
for i in $(seq 1 ${QUEIMADORES:-8}); do ( while :; do :; done ) & PIDS+=($!); done
for b in $BINS; do ( while :; do "$b" >/dev/null 2>&1; sleep 0.2; done ) & PIDS+=($!); done
sleep 5
echo "carga: ${QUEIMADORES:-8} queimadores + ${#BINS} laços de conformidade fora do cargo (${BINS[*]:t}); load = $(uptime | sed 's/.*averages: //')" > "$LOG"
echo "processos do usuario com a carga ligada: $(ps -u $(id -u) | wc -l | tr -d ' ')" >> "$LOG"

INI=$(date +%s.%N)
"$@" >> "$LOG" 2>&1
SAIDA=$?
FIM=$(date +%s.%N)
kill ${PIDS[@]} 2>/dev/null
for b in $BINS; do pkill -f "$b" 2>/dev/null; done
wait 2>/dev/null
PAREDE=$(echo "$FIM - $INI" | bc)
echo "=== ROTULO=$ROTULO SAIDA=$SAIDA PAREDE=${PAREDE}s ===" >> "$LOG"
echo "ROTULO=$ROTULO SAIDA=$SAIDA PAREDE=${PAREDE}s"
