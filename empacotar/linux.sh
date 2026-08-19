#!/bin/sh
# Empacota o `.deb` do Linux sem o GitHub Actions, num contêiner.
#
# Faz o mesmo que o job `linux` de `.github/workflows/release.yml`. Roda de
# qualquer sistema que tenha Docker — a compilação acontece dentro do contêiner,
# e nada além do `entrega/` sai dele.
#
#   ./empacotar/linux.sh 0.1.2
#
# **Numa máquina Apple Silicon isto é emulado.** `tauri.linux.conf.json` fixa os
# caminhos do `.deb` em `x86_64-unknown-linux-gnu`, então o alvo é x86_64 e o
# Docker o executa por emulação — conte com algo entre trinta e noventa minutos,
# contra dez ou quinze num x86_64 nativo. Não há defeito nisso; é o preço de
# compilar para outra arquitetura.
#
# O `target/` do repositório não é reaproveitado: ele foi construído para o
# sistema desta máquina, e misturar os dois só produz recompilação confusa. O
# contêiner usa `target-linux/`, que fica ao lado.
#
# Para que o `.deb` saia atualizável, exporte a chave do projeto antes:
#
#   export TAURI_SIGNING_PRIVATE_KEY="$(cat ~/.tauri/seele.key)"
#   export TAURI_SIGNING_PRIVATE_KEY_PASSWORD=…
#
# Ela atravessa para dentro do contêiner. Sem ela sai o `.deb` de sempre.
# `docs/assinatura-e-atualizacao.md` diz de onde a chave vem.

set -eu

VERSAO="${1:-}"
if [ -z "$VERSAO" ]; then
    echo "uso: $0 <versão>    (por exemplo: $0 0.1.2)" >&2
    exit 1
fi

# A mesma regra do workflow: o formato do instalador não representa
# pré-lançamento por extenso, e conferir aqui custa um segundo em vez de uma
# compilação inteira.
if ! printf '%s' "$VERSAO" | grep -Eq '^[0-9]+\.[0-9]+\.[0-9]+(-[0-9]+)?$'; then
    echo "a versão «${VERSAO}» não serve para o instalador." >&2
    echo "Aceito: X.Y.Z, ou X.Y.Z-N com N só de dígitos." >&2
    exit 1
fi

RAIZ="$(CDPATH='' cd -- "$(dirname -- "$0")/.." && pwd)"

if ! command -v docker >/dev/null 2>&1; then
    echo "este script precisa do Docker. Sem ele, compile num Linux x86_64 de verdade" >&2
    echo "seguindo os mesmos passos — estão no corpo do contêiner, logo abaixo." >&2
    exit 1
fi

echo "→ compilando em contêiner x86_64 (emulado se esta máquina for ARM)"

docker run --rm -i \
    --platform linux/amd64 \
    -v "$RAIZ:/work" \
    -w /work \
    -e CARGO_TARGET_DIR=/work/target-linux \
    -e VERSAO="$VERSAO" \
    -e DEBIAN_FRONTEND=noninteractive \
    -e TAURI_SIGNING_PRIVATE_KEY="${TAURI_SIGNING_PRIVATE_KEY:-}" \
    -e TAURI_SIGNING_PRIVATE_KEY_PASSWORD="${TAURI_SIGNING_PRIVATE_KEY_PASSWORD:-}" \
    rust:1-bookworm sh -s <<'DENTRO'
set -eu

# As dependências de `.github/actions/deps-linux`, **mais** as que aquela ação
# não precisa listar. Ela roda no runner do GitHub, que já vem com clang, cmake e
# um compilador C; a imagem `rust:bookworm` vem só com o compilador. Sem
# `libclang-dev` a compilação chega até a crate do Opus e morre em `bindgen`
# com "Unable to find libclang" — depois de vários minutos, que sob emulação
# são muitos.
#
# `libwebkit2gtk` é o motor que o Tauri usa no Linux; `libasound2` é o que o
# áudio abre.
apt-get update -qq
apt-get install --no-install-recommends -y -qq \
    pkg-config libasound2-dev libwebkit2gtk-4.1-dev libgtk-3-dev \
    libayatana-appindicator3-dev librsvg2-dev libsoup-3.0-dev \
    libjavascriptcoregtk-4.1-dev libxdo-dev file \
    clang libclang-dev cmake >/dev/null

# Absoluto, e não relativo: este script troca de diretório mais adiante, e o
# `trap` roda com o diretório que estiver valendo na saída. Com caminho relativo
# a restauração erra o alvo e falha caladamente — foi o que aconteceu, e o
# `tauri.conf.json` ficou com a versão do release gravada no repositório.
CONFIG=/work/apps/seele-app/tauri.conf.json
cp "$CONFIG" /tmp/tauri.conf.json.original
trap 'cp /tmp/tauri.conf.json.original "$CONFIG"' EXIT INT TERM

# A versão que aparece no pacote. Sem isto o `.deb` diria 0.0.0, que é a versão
# do workspace.
python3 - <<PY
import json, os, pathlib
caminho = pathlib.Path("$CONFIG")
config = json.loads(caminho.read_text())
config["version"] = "$VERSAO"

# No Linux o `.deb` **é** o pacote de atualização: o mesmo arquivo que uma
# pessoa baixaria à mão. O que falta é a assinatura ao lado dele, e ela só sai
# com as duas metades da chave do projeto — a pública do repositório e a privada
# do ambiente.
privada = os.environ.get("TAURI_SIGNING_PRIVATE_KEY", "").strip()
publica = config.get("plugins", {}).get("updater", {}).get("pubkey", "").strip()
if privada and publica:
    config["bundle"]["createUpdaterArtifacts"] = True
    print("com pacote de atualização")
elif privada or publica:
    # Qual metade falta, e o que fazer — não só que falta uma.
    #
    # A mensagem antiga dizia «só metade da chave» e parava aí. Quem a lia sabia
    # que algo estava incompleto e não sabia o quê nem onde, o que é a mesma
    # falha que este produto passou o dia consertando na tela: a mensagem tem a
    # informação e não a entrega.
    if privada:
        print("a chave privada está no ambiente e a pública não está no "
              "tauri.conf.json (plugins.updater.pubkey).")
    else:
        print("a chave pública está no tauri.conf.json e a privada não está "
              "no ambiente. Para assinar:")
        print("  export TAURI_SIGNING_PRIVATE_KEY=\"$(cat ~/.tauri/seele.key)\"")
        print("  export TAURI_SIGNING_PRIVATE_KEY_PASSWORD=…")
    print("Este .deb funciona normal; ele só não serve de origem para atualização.")

caminho.write_text(json.dumps(config, indent=2, ensure_ascii=False) + "\n")
print("versão gravada:", config["version"])
PY

# A CLI primeiro, como no workflow: é o produto principal e o que menos pode
# falhar.
echo "→ compilando seeled e plug"
cargo build --release --bin seeled --bin plug

# No Linux os dois **não** vão embutidos como acompanhantes do Tauri: o
# `deb.files` de `tauri.linux.conf.json` os instala em `/usr/bin`, e é por isso
# que o empacotamento abaixo não recebe `--config tauri.release.conf.json`.
# Receber os dois faria o `.deb` carregar os binários duas vezes.
mkdir -p apps/seele-app/binaries
for b in plug seeled; do
    cp "/work/target-linux/release/$b" "apps/seele-app/binaries/$b-x86_64-unknown-linux-gnu"
done

echo "→ instalando a CLI do Tauri"
cargo install tauri-cli --version "^2" --locked --quiet

# `.deb` e não AppImage: é o único formato que instala `plug` e `seeled` em
# `/usr/bin`, e o pedido era um arquivo que traga as duas metades.
echo "→ empacotando o .deb"
cd apps/seele-app
cargo tauri build --bundles deb
cd /work

DESTINO=entrega
mkdir -p "$DESTINO"
# Filtrado pela versão, como o irmão do macOS.
#
# Sem isso ele conta todo `.deb` que já passou pela árvore: um `SEELE_0.2.0`
# esquecido no `target-linux` reprovou o empacotamento do 0.6.4 depois de a
# compilação inteira ter dado certo — e depois de a assinatura ter saído.
#
# O macOS foi consertado assim quando o mesmo defeito apareceu lá, e o comentário
# de lá até cita o irmão do Windows tendo reprovado do mesmo jeito. Este ficou
# para trás; três scripts com o mesmo defeito e dois consertos é como o terceiro
# espera o pior momento.
padrao="*_${VERSAO}_*.deb"
encontrados=$(find target-linux -type f -name "$padrao" | wc -l | tr -d ' ')
if [ "$encontrados" != "1" ]; then
    echo "esperava exatamente um .deb de ${VERSAO} e achei $encontrados" >&2
    find target-linux -type f -name "$padrao" >&2
    exit 1
fi
find target-linux -type f -name "$padrao" -exec cp {} "$DESTINO/" \;

# A assinatura do atualizador, quando este empacotamento teve a chave.
find target-linux -type f -name "${padrao}.sig" -exec cp {} "$DESTINO/" \; 2>/dev/null || true

# O arquivo da CLI, que é o que o `install.sh` baixa.
tar -czf "$DESTINO/seele-cli-$VERSAO-linux.tar.gz" -C target-linux/release seeled plug

echo
echo "--- entrega ---"
cd "$DESTINO"
sha256sum *
DENTRO

echo
echo "pronto. Os arquivos estão em entrega/, e as somas acima são as que devem"
echo "constar do SHA256SUMS."
if ls "$RAIZ"/entrega/*.deb.sig >/dev/null 2>&1; then
    echo
    echo "Depois de reunir os três sistemas em entrega/, monte o manifesto:"
    echo "  python3 empacotar/manifesto.py entrega v$VERSAO"
    echo "Sem ele o botão de atualizar do app não acha este release."
fi
