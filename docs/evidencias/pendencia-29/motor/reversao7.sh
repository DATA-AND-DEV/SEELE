#!/bin/zsh
# Ciclo de reversão nº 7 — o que a segunda revisão independente cobrou:
#  * sobre o código REALMENTE entregue (backup vaga_ARMADO7.backup.rs);
#  * a reprovação por PRAZO sob `cargo test --workspace`, com a permissão
#    desarmada;
#  * sob a carga que de fato reproduz a §29 — servidores QUIC concorrentes fora
#    do cargo (`rodada4.sh`, já sem a bomba de forks do `rodada3.sh`) —, porque
#    está medido que queimador de processador sozinho NÃO a reproduz;
#  * restauração conferida byte a byte contra o backup.
set -u
export PATH="$HOME/.cargo/bin:$PATH"
W="/Users/dev-alexandre/Documents/Obsidian Vault/Órbita/.orbita/worktrees/299c2412-1abd-4516-9748-226ce1950e3e"
V="$W/crates/seele-conformance/tests/vaga/mod.rs"
B=/tmp/o29/vaga_ARMADO7.backup.rs
cd "$W"
while pgrep -x cargo >/dev/null 2>&1; do sleep 10; done

echo "--- ANTES DE DESARMAR ---"; shasum -a 256 "$V" "$B"
perl -pi -e 's/^const VAGAS: usize = 1;$/const VAGAS: usize = usize::MAX;/' "$V"
grep -n 'const VAGAS' "$V"
cargo test --workspace --no-run >/dev/null 2>&1

for r in 1 2 3 4 5; do
  while pgrep -x cargo >/dev/null 2>&1; do sleep 5; done
  QUEIMADORES=8 /tmp/o29/rodada4.sh "revws7_${r}" cargo test --workspace
  S=$(grep -o 'SAIDA=[0-9]*' /tmp/o29/revws7_${r}.log | tail -1 | cut -d= -f2)
  if [[ "$S" != "0" ]]; then echo "REPROVOU DESARMADO na rodada $r"; break; fi
done
echo "--- fim das rodadas desarmadas ---"

cp "$B" "$V"
echo "--- RESTAURACAO ---"; shasum -a 256 "$B" "$V"
cmp "$B" "$V" && echo "RESTAURADO: identico byte-a-byte ao backup"
grep -n 'const VAGAS' "$V"
cargo test --workspace --no-run >/dev/null 2>&1
QUEIMADORES=8 /tmp/o29/rodada4.sh pos_rev7 cargo test --workspace
echo "FIM DO CICLO 7"
