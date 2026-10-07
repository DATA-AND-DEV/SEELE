#!/bin/sh
# Instala o `seeled` a partir de um release do GitHub.
#
#   curl -fsSL https://raw.githubusercontent.com/DATA-AND-DEV/SEELE/main/install.sh | sh
#
# Antes de rodar isso: você está prestes a executar um script que veio da rede.
# Num produto cujo argumento é não depender de terceiros, isso merece um
# segundo de atenção. Duas alternativas melhores, se preferir:
#
#   1. Baixe, leia, e só então rode:
#        curl -fsSLO https://raw.githubusercontent.com/DATA-AND-DEV/SEELE/main/install.sh
#        less install.sh && sh install.sh
#
#   2. Não use script nenhum: pegue o `.tar.gz` na aba Releases de
#      DATA-AND-DEV/SEELE-RELEASES, confira a soma contra o `SHA256SUMS`
#      publicado ao lado, e descompacte onde quiser.
#
# O que este script faz, e nada além: descobre o sistema, baixa a lista de
# somas da versão pedida e confere nela que a versão publica o servidor para
# este sistema, baixa o pacote, **confere a soma SHA-256** contra aquela lista,
# e copia o binário para um diretório. Não mexe no seu shell, não escreve em
# `/usr`, não pede `sudo`, não manda nada para lugar nenhum.
#
# Variáveis:
#   SEELE_VERSION  versão a instalar (padrão: a última publicada)
#   SEELE_BIN      onde instalar (padrão: ~/.local/bin)
#   SEELE_BASE     de onde baixar (padrão: os releases de
#                  DATA-AND-DEV/SEELE-RELEASES). Serve para espelho interno — e
#                  é o que torna este script testável, em vez de só escrito.

set -eu

# **Dois repositórios, e não um.** As versões moram em SEELE-RELEASES: é lá que
# o `empacotar/publicar.sh` publica, e é para lá que o atualizador do app
# aponta primeiro. O código mora em SEELE, e serve aqui só para a dica de
# compilar.
#
# Antes eram o mesmo nome numa variável só, e este script baixava do
# repositório do código, onde a última versão publicada é a v0.10.0 (medido em
# 04/10/2026): a linha do README instalava, sem avisar, um servidor que nenhum
# cliente 0.15 alcança. O teste
# `os_instaladores_de_uma_linha_baixam_de_onde_o_publicar_publica`, do `xtask`,
# exige que esta casa esteja entre as do `publicar.sh`.
REPO_DAS_VERSOES="DATA-AND-DEV/SEELE-RELEASES"
REPO_DO_CODIGO="DATA-AND-DEV/SEELE"
BIN="${SEELE_BIN:-$HOME/.local/bin}"

erro() {
    printf '\nerro: %s\n' "$1" >&2
    exit 1
}

precisa() {
    command -v "$1" >/dev/null 2>&1 || erro "preciso de \`$1\` e não achei."
}

# O que dizer a quem não tem pacote: compilar do código-fonte, e como.
COMPILAR="Compile do código-fonte: git clone https://github.com/$REPO_DO_CODIGO && cd SEELE && cargo build --release --bin seeled"

precisa curl
precisa tar

# ---------------------------------------------------------------- plataforma

case "$(uname -s)" in
    Linux)  SISTEMA=linux ;;
    Darwin) SISTEMA=macos ;;
    *)      erro "sistema não suportado por este script: $(uname -s).
       No Windows use o install.ps1, ou baixe o .zip na aba Releases de
       $REPO_DAS_VERSOES." ;;
esac

# ------------------------------------------------------------------- versão

if [ -n "${SEELE_VERSION:-}" ]; then
    VERSAO="$SEELE_VERSION"
else
    printf 'procurando a última versão... '
    # Sem `jq`: a API devolve o campo numa linha previsível, e depender de mais
    # uma ferramenta para ler um número seria pior.
    VERSAO=$(curl -fsSL "https://api.github.com/repos/$REPO_DAS_VERSOES/releases/latest" 2>/dev/null \
        | sed -n 's/.*"tag_name": *"\([^"]*\)".*/\1/p' | head -n1)
    [ -n "$VERSAO" ] || erro "não achei nenhuma versão publicada em $REPO_DAS_VERSOES.

       Se ainda não houver release publicado (um rascunho não conta), ou se a
       API do GitHub não respondeu, este script não tem de onde baixar.
       $COMPILAR"
    printf '%s\n' "$VERSAO"
fi

NUMERO="${VERSAO#v}"
PACOTE="seele-cli-${NUMERO}-${SISTEMA}.tar.gz"
BASE="${SEELE_BASE:-https://github.com/$REPO_DAS_VERSOES/releases/download/$VERSAO}"

TRABALHO=$(mktemp -d)
# Sai limpo mesmo se algo falhar no meio: um diretório temporário com binários
# pela metade é o tipo de coisa que confunde na próxima tentativa.
trap 'rm -rf "$TRABALHO"' EXIT INT TERM

# ------------------------------------------------- o que esta versão publica

# **A lista de somas antes do pacote.** Ela sai em todo release e diz o que ele
# publica. Baixar o pacote primeiro respondia a falta dele com «não consegui
# baixar», que não diz se foi a rede, o nome ou a versão — e é a falta que
# acontece: a v0.15.0 não publica pacote do Linux (medido em 04/10/2026).
#
# **E a falta da lista não é uma coisa só.** O código de saída do curl separa a
# versão que não existe (22, um HTTP de 400 para cima; 37, o arquivo que não
# está lá quando a base é um `file://`) da rede que não respondeu (todo o
# resto). Uma frase só para os dois dizia «este release não publica
# SHA256SUMS» a quem estava sem rede, e mandava baixar sem conferir.
printf 'conferindo o que a versão %s publica... ' "$VERSAO"
codigo=0
curl -fsSL -o "$TRABALHO/SHA256SUMS" "$BASE/SHA256SUMS" || codigo=$?
case "$codigo" in
    0) : ;;
    22|37) erro "não achei $BASE/SHA256SUMS: a versão $VERSAO não existe ali, ou não publica a lista de somas.

       Confira o nome da versão em https://github.com/$REPO_DAS_VERSOES/releases.
       Sem soma não há o que conferir, e instalar um binário sem conferir é
       exatamente o que este script deveria evitar. Baixe manualmente só se
       aceitar o risco conscientemente." ;;
    *) erro "não consegui falar com $BASE (o curl saiu com $codigo).

       Confira a rede e tente de novo. Nada foi instalado." ;;
esac

ESPERADA=$(grep " \*\{0,1\}$PACOTE\$" "$TRABALHO/SHA256SUMS" | awk '{print $1}' | head -n1)
[ -n "$ESPERADA" ] || erro "a versão $VERSAO não publica o servidor para $SISTEMA.
       $COMPILAR"
printf '%s\n' "$PACOTE"

# **A arquitetura, depois da lista.** O pacote do macOS é só da arquitetura de
# quem o compila — `empacotar/macos.sh` empacota o `seeled` do alvo daquela
# máquina —, e quem publica compila num Mac Apple Silicon. O do Linux, quando
# houver, sai x86_64.
if [ "$SISTEMA" = macos ]; then
    # Pelo `hw.optional.arm64`, e não pelo `uname -m`: num terminal sob
    # Rosetta, o `uname -m` de um Mac Apple Silicon diz x86_64, e recusaria a
    # máquina certa. Num Mac Intel o `sysctl -in` devolve vazio, que é «não».
    if [ "$(sysctl -in hw.optional.arm64 2>/dev/null || true)" != 1 ]; then
        erro "o pacote do macOS desta versão é só para Apple Silicon, e este Mac é Intel.
       $COMPILAR"
    fi
fi
if [ "$SISTEMA" = linux ]; then
    case "$(uname -m)" in
        x86_64|amd64) : ;;
        *) erro "no Linux só há pacote para x86_64, e esta máquina é $(uname -m).
       $COMPILAR" ;;
    esac
fi

# --------------------------------------------------------------------- baixa

printf 'baixando %s\n' "$PACOTE"
curl -fsSL -o "$TRABALHO/$PACOTE" "$BASE/$PACOTE" \
    || erro "não consegui baixar $PACOTE, que o SHA256SUMS da versão $VERSAO lista."

# ------------------------------------------------------------------ confere

printf 'conferindo a soma... '
if command -v sha256sum >/dev/null 2>&1; then
    OBTIDA=$(sha256sum "$TRABALHO/$PACOTE" | awk '{print $1}')
elif command -v shasum >/dev/null 2>&1; then
    OBTIDA=$(shasum -a 256 "$TRABALHO/$PACOTE" | awk '{print $1}')
else
    erro "preciso de \`sha256sum\` ou \`shasum\` para conferir a soma."
fi

if [ "$ESPERADA" != "$OBTIDA" ]; then
    erro "A SOMA NÃO CONFERE.

       esperada: $ESPERADA
       obtida:   $OBTIDA

       O arquivo baixado não é o que foi publicado. Não instale.
       Pode ser corrupção no caminho — ou não."
fi
printf 'confere\n'

# ------------------------------------------------------------------ instala

tar -xzf "$TRABALHO/$PACOTE" -C "$TRABALHO"
mkdir -p "$BIN"

for programa in seeled; do
    [ -f "$TRABALHO/$programa" ] || erro "o pacote não traz \`$programa\`."
    # Copia e depois renomeia: substituir um binário em uso falha no meio se
    # for escrito por cima.
    cp "$TRABALHO/$programa" "$BIN/$programa.novo"
    chmod +x "$BIN/$programa.novo"
    mv -f "$BIN/$programa.novo" "$BIN/$programa"
done

# O macOS põe em quarentena o que veio da rede e recusa abrir sem isso.
if [ "$SISTEMA" = macos ] && command -v xattr >/dev/null 2>&1; then
    xattr -d com.apple.quarantine "$BIN/seeled" 2>/dev/null || true
fi

printf '\ninstalado em %s\n' "$BIN"
printf '  seeled  o servidor\n'

case ":$PATH:" in
    *":$BIN:"*) ;;
    *)
        printf '\n  %s não está no seu PATH. Acrescente:\n' "$BIN"
        printf '    export PATH="%s:$PATH"\n' "$BIN"
        ;;
esac

printf '\npara começar:\n'
printf '  seeled 0.0.0.0:8383      numa máquina\n'
printf "  e conecte pelo app SEELE da outra\n"
printf "\npara remover: rm %s/seeled\n" "$BIN"
