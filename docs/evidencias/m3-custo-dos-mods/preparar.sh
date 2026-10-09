#!/bin/sh
# M3 da 0.16 — monta a pasta de trabalho da medição, do zero.
#
#   sh preparar.sh <pasta-de-trabalho> [commit]      (commit padrão: 54f542f, a v0.15.1)
#
# Faz, nesta ordem, e sem tocar no repositório de onde é chamado:
#   1. copia o código do commit por `git archive` para <pasta>/fonte;
#   2. põe ali os dois exemplos da medição (o servidor e o falante) e os compila
#      em <pasta>/alvo, com `--release`;
#   3. compila o utilitário de acessibilidade (`ax`);
#   4. baixa do indexador os três pacotes oficiais que a v0.15.1 escolhe
#      (PERFIS 3.3.0, MESA 3.1.0, ESTILO 3.1.0) e confere o hash de cada um
#      contra o catálogo, pelo próprio servidor da medição;
#   5. copia o roteiro para <pasta>/roteiro.
#
# O cache de pacotes do app (<pasta>/cache/mod-packages) é opcional: sem ele,
# o app baixa os pacotes do indexador a cada cenário, como baixaria para
# qualquer pessoa. Para criá-lo, rode um cenário com MODs uma vez e copie o
# `casa/mod-packages` dele para <pasta>/cache/.
set -eu
PASTA=$(cd "$(dirname "$1")" && pwd)/$(basename "$1")
COMMIT=${2:-54f542f}
AQUI=$(cd "$(dirname "$0")" && pwd)
REPO=$(git -C "$AQUI" rev-parse --show-toplevel)
mkdir -p "$PASTA/fonte" "$PASTA/roteiro" "$PASTA/pacotes" "$PASTA/medidas"

git -C "$REPO" archive "$COMMIT" | tar -x -C "$PASTA/fonte"
cp "$AQUI/m3-servidor.rs" "$PASTA/fonte/crates/seele-server/examples/"
mkdir -p "$PASTA/fonte/crates/seele-conformance/examples"
cp "$AQUI/m3-falante.rs" "$PASTA/fonte/crates/seele-conformance/examples/"
( cd "$PASTA/fonte" \
  && CARGO_TARGET_DIR="$PASTA/alvo" cargo build --release -p seele-server --example m3-servidor \
  && CARGO_TARGET_DIR="$PASTA/alvo" cargo build --release -p seele-conformance --example m3-falante )

swiftc -O "$AQUI/ax.swift" -o "$PASTA/ax"
swiftc -O "$AQUI/sessao.swift" -o "$PASTA/sessao"
cp "$AQUI/m3.py" "$AQUI/xctrace_tabela.py" "$AQUI/campanha.sh" "$PASTA/roteiro/"

curl -sSf -o "$PASTA/pacotes/catalogo.json" https://mods.seele.app.br/catalogo.json
python3 -I - "$PASTA" <<'PY'
import json, os, subprocess, sys
pasta = sys.argv[1]
catalogo = json.load(open(os.path.join(pasta, "pacotes/catalogo.json")))
escolha = {"seele/perfis": "3.3.0", "seele/mesa": "3.1.0", "seele/estilo": "3.1.0"}
for mod in catalogo["mods"]:
    versao = escolha.get(mod["id"])
    if not versao:
        continue
    v = next(x for x in mod["versoes"] if x["versao"] == versao)
    destino = os.path.join(pasta, "pacotes", mod["id"].replace("/", "-") + "-" + versao)
    for relativo in v["arquivos"]:
        caminho = os.path.join(destino, relativo)
        os.makedirs(os.path.dirname(caminho), exist_ok=True)
        subprocess.run(["curl", "-sSf", "-o", caminho,
                        f"https://mods.seele.app.br/mods/{mod['id']}/{versao}/{relativo}"], check=True)
    print(mod["id"], versao, "hash do catálogo", v["hash"])
PY
# o hash calculado pelo servidor da medição tem de bater com o do catálogo
"$PASTA/alvo/release/examples/m3-servidor" 8391 "$PASTA/conferencia" \
  "$PASTA/pacotes/seele-perfis-3.3.0" "$PASTA/pacotes/seele-mesa-3.1.0" "$PASTA/pacotes/seele-estilo-3.1.0" \
  2>/dev/null | head -1 &
SERVIDOR=$!
sleep 3
kill "$SERVIDOR" 2>/dev/null || true
pkill -f "m3-servidor 8391 $PASTA/conferencia" 2>/dev/null || true
rm -rf "$PASTA/conferencia"
echo "pronto: python3 -I $PASTA/roteiro/m3.py sem-mod   ou   sh $PASTA/roteiro/campanha.sh"
