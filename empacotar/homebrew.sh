#!/bin/sh
# Gera a fórmula de Homebrew do `seeled` a partir do que já está em `entrega/`.
#
# ---- por que gerado, e não escrito à mão ----
#
# Uma fórmula carrega a versão e a soma SHA256 de cada pacote, e as três coisas
# mudam a cada release. Uma fórmula escrita à mão nasce certa e fica errada no
# release seguinte — e uma fórmula errada não avisa: o `brew install` baixa,
# confere a soma, e falha com uma mensagem sobre integridade que parece
# adulteração. Este projeto já pagou duas vezes hoje por documento que descrevia
# como vigente uma decisão revogada; uma fórmula estática seria a terceira.
#
# Então ela sai de onde os fatos estão: `entrega/SHA256SUMS`, que o
# `publicar.sh` acabou de gerar, e a versão que ele recebeu.
#
# ---- o que o Homebrew resolve, e o que não ----
#
# **Resolve** o alerta do macOS para o `seeled`. Binário instalado por `brew` não
# recebe o atributo `com.apple.quarantine`, que é o que faz o Gatekeeper
# reclamar — e o `install.sh` hoje tira aquele atributo na unha, na linha 150.
# Com a fórmula, o contorno deixa de ser necessário em vez de ser automatizado.
#
# **Não resolve** o alerta do aplicativo gráfico. Um cask aplica quarentena por
# padrão, e ali o conserto é notarização (Apple Developer Program). Trocar o
# `curl | sh` por um cask seria trocar um contorno por outro.
#
# ---- uso ----
#
#   empacotar/homebrew.sh 0.7.0 > seeled.rb
#
# O destino é um tap — um repositório `homebrew-seele` — e não o homebrew-core,
# que exige notoriedade que este projeto ainda não tem. Ver `docs/homebrew.md`.

set -eu

VERSAO="${1:-}"
if [ -z "$VERSAO" ]; then
    echo "uso: $0 <versão sem o v>   (ex.: $0 0.7.0)" >&2
    exit 2
fi

RAIZ=$(CDPATH='' cd -- "$(dirname -- "$0")/.." && pwd)
SOMAS="$RAIZ/entrega/SHA256SUMS"

if [ ! -f "$SOMAS" ]; then
    echo "não achei $SOMAS." >&2
    echo "Rode o empacotar/publicar.sh antes: é ele que monta entrega/ e soma." >&2
    exit 1
fi

# A soma de um pacote, pelo nome. Vazio quando o pacote não foi montado — o
# `publicar.sh` aceita `--pular`, e um release sem macOS é um estado normal.
soma() {
    awk -v alvo="$1" '$2 == alvo || $2 == "*" alvo { print $1; exit }' "$SOMAS"
}

PACOTE_MAC="seele-cli-${VERSAO}-macos.tar.gz"
PACOTE_LINUX="seele-cli-${VERSAO}-linux.tar.gz"
SOMA_MAC=$(soma "$PACOTE_MAC")
SOMA_LINUX=$(soma "$PACOTE_LINUX")

if [ -z "$SOMA_MAC" ] && [ -z "$SOMA_LINUX" ]; then
    echo "nenhum pacote de CLI em entrega/ para a versão $VERSAO." >&2
    echo "Procurei por $PACOTE_MAC e $PACOTE_LINUX." >&2
    exit 1
fi

BASE="https://github.com/DATA-AND-DEV/SEELE/releases/download/v${VERSAO}"

cat <<FORMULA
# Gerado por \`empacotar/homebrew.sh\`. Não edite à mão: a versão e as somas
# mudam a cada release, e uma edição manual some no próximo.
class Seeled < Formula
  desc "Servidor de voz do SEELE: o daemon que se hospeda na própria casa"
  homepage "https://github.com/DATA-AND-DEV/SEELE"
  version "${VERSAO}"
  license :cannot_represent

FORMULA

if [ -n "$SOMA_MAC" ]; then
    cat <<FORMULA
  on_macos do
    url "${BASE}/${PACOTE_MAC}"
    sha256 "${SOMA_MAC}"
  end

FORMULA
fi

if [ -n "$SOMA_LINUX" ]; then
    cat <<FORMULA
  on_linux do
    url "${BASE}/${PACOTE_LINUX}"
    sha256 "${SOMA_LINUX}"
  end

FORMULA
fi

cat <<'FORMULA'
  def install
    bin.install "seeled"
  end

  # Um teste que **não** liga o daemon de verdade.
  #
  # `brew test` roda numa máquina que não é a de quem instalou, sem porta livre
  # garantida e sem rede prometida. Subir um servidor QUIC ali produziria uma
  # falha que não é sobre a instalação — que é o único assunto deste teste.
  #
  # `--ajuda` e não `--version`: **o `seeled` não tem `--version`**. Conferido
  # em 2026-08-31, e não é omissão deste arquivo — está registrado como
  # pendência. Enquanto não tiver, o que prova que o binário certo foi instalado
  # e executa nesta arquitetura é ele responder à ajuda com a própria primeira
  # linha. Um pacote da arquitetura errada não chega até aqui.
  test do
    assert_match "seeled", shell_output("#{bin}/seeled --ajuda")
  end
end
FORMULA
