#!/usr/bin/env python3
"""Carrega a casca num navegador de verdade e diz o que estourou.

**Por que isto existe.** Os 156 guardas de `apps/seele-app/tests/frontend.rs`
leem a casca como *texto*: eles conferem que um id existe, que uma frase está
escrita, que uma classe não é definida duas vezes. Nenhum deles **executa** os
catorze scripts — e três defeitos passaram por todos eles ao mesmo tempo, em
versões que chegaram ao campo:

- um `let` com o nome de uma `function` de outro arquivo, que é `SyntaxError`
  no arquivo inteiro (os scripts dividem um escopo global só);
- uma chamada solta no topo de um script para uma função que não existe mais,
  que mata tudo o que era registrado depois dela — inclusive o `click` do
  `CONECTAR`;
- um valor calculado e nunca lido, que fazia hospedar continuar pedindo para
  conferir a própria chave.

Os dois primeiros ganharam guarda estático depois. O terceiro não tem como ter:
`const direto = …` seguido de um `if` que testa outra coisa é código válido e
plausível. Só carregar e apertar acha.

**O duble.** `window.__TAURI__` não existe fora do app, e sem ele `base.js`
morre na primeira linha. O duble aqui responde o suficiente para a casca chegar
à sessão; ele **não** finge um `Snapshot` fiel, e por isso o roteiro anula
`desenhar` e `atualizar`, que pintam a partir dele. O que este aparelho mede é
qual tela fica na frente e o que estoura no caminho — não o desenho, que é o que
uma foto responde melhor.

**A cópia.** A casca é copiada para uma pasta temporária antes de ser mexida.
Escrever a página de teste dentro de `ui/` já reprovou um guarda que lê tudo o
que mora lá — e um arquivo de teste esquecido naquela pasta é um arquivo que vai
para o instalador.

Uso:
    python3 tools/carga-da-casca.py            # só a carga
    python3 tools/carga-da-casca.py --roteiro tools/roteiros/hospedar.js
"""

import argparse
import pathlib
import re
import shutil
import subprocess
import sys
import tempfile

RAIZ = pathlib.Path(__file__).resolve().parent.parent
CASCA = RAIZ / "apps" / "seele-app" / "ui"

DUBLE = """<script>
// O duble do Tauri. Responde o bastante para a casca andar; ver o cabeçalho de
// `tools/carga-da-casca.py` sobre o que ele deliberadamente não finge.
const SEELE_RESPOSTAS = {
  hospedar: { aqui: "127.0.0.1:8383", convite: "seele://127.0.0.1:8383?fp=abc",
              alcance: "SoRedeLocal", porta_recusada: null, encontro_recusado: null },
};
const SEELE_QUADRO = {
  server: "AQUI", ended: null, people: [], voice_rooms: [], channels: [], messages: [],
  link_state: "Verified", nickname: "eu", muted: false, total_isolation: false,
  voice_mode: "Open", audio_available: true, speaking: false, revision: 1, tela: null,
  my_voice_room: null, may_remove_message: false, permissions: {},
  telemetry: { signal: 100, latency_ms: 0, jitter_ms: 0, loss: 0, codec: "OPUS", path: "LOCAL" },
};
SEELE_RESPOSTAS.snapshot = SEELE_QUADRO;
SEELE_RESPOSTAS.connect = { snapshot: SEELE_QUADRO, veredito: null };
window.__SEELE_CHAMADAS = [];
window.__TAURI__ = {
  core: {
    invoke: async (cmd, args) => {
      window.__SEELE_CHAMADAS.push(cmd);
      if (cmd in SEELE_RESPOSTAS) return SEELE_RESPOSTAS[cmd];
      if (/conhecid|fontes|lista|dispositiv|visitad|salas|canais|pessoas/i.test(cmd)) return [];
      if (/apelido|nickname|preferenc|link|caminho/i.test(cmd)) return "";
      return null;
    },
    convertFileSrc: (p) => p,
  },
  event: { listen: async () => () => {}, emit: async () => {} },
  window: { getCurrentWindow: () => ({
    minimize: async () => {}, toggleMaximize: async () => {}, close: async () => {},
    isFullscreen: async () => false, setFullscreen: async () => {},
    onCloseRequested: async () => () => {}, listen: async () => () => {},
  }) },
  webviewWindow: { getCurrentWebviewWindow: () => ({ listen: async () => () => {} }) },
  dialog: { open: async () => null, save: async () => null, message: async () => {} },
  fs: { readFile: async () => new Uint8Array() },
  updater: { check: async () => null },
};
</script>
"""

CABECA_DO_ROTEIRO = """<script>
window.__SEELE_RELATO = [];
const relatar = (linha) => window.__SEELE_RELATO.push(linha);
const espera = (ms) => new Promise((r) => setTimeout(r, ms));
const visivel = (id) => {
  const el = document.getElementById(id);
  return el ? (el.hidden ? "escondida" : "VISIVEL") : "AUSENTE";
};
const telas = (quando) =>
  `${quando}: boot=${visivel("tela-boot")} auth=${visivel("tela-auth")} sessao=${visivel("tela-sessao")}`;
setTimeout(async () => {
  try {
    // A casca desenha a sessão inteira a partir do Snapshot, e o duble não tem
    // como fabricar um fiel. Anulados: o que se mede aqui é o caminho.
    desenhar = () => {};
    atualizar = async () => {};
"""

PE_DO_ROTEIRO = """
  } catch (erro) {
    relatar("ROTEIRO ESTOUROU: " + erro);
  }
  console.log("SEELE-RELATO " + window.__SEELE_RELATO.join(" | "));
}, 800);
</script>
"""


def navegador() -> str:
    """O primeiro `chrome-headless-shell` que este computador já tem."""
    for raiz in [
        pathlib.Path.home() / "Library" / "Caches" / "ms-playwright",
        pathlib.Path.home() / ".cache" / "ms-playwright",
        pathlib.Path.home() / ".cache" / "puppeteer",
    ]:
        if not raiz.exists():
            continue
        achados = sorted(raiz.rglob("chrome-headless-shell"))
        if achados:
            return str(achados[-1])
    print(
        "não achei um `chrome-headless-shell`. Playwright ou Puppeteer instalam um;\n"
        "`npx playwright install chromium` basta.",
        file=sys.stderr,
    )
    raise SystemExit(2)


def main() -> int:
    argumentos = argparse.ArgumentParser(description=__doc__)
    argumentos.add_argument("--roteiro", type=pathlib.Path, default=None)
    argumentos.add_argument("--janela", default="1440,900")
    # Para comparar contra uma versão antiga: aponte para o `ui/` de um
    # `git worktree`, e o mesmo roteiro roda contra o passado.
    argumentos.add_argument("--casca", type=pathlib.Path, default=CASCA)
    # A foto é tirada sempre; isto só a guarda em vez de deixá-la sumir com a
    # pasta temporária. Ver o que a tela ficou é metade do diagnóstico.
    argumentos.add_argument("--foto", type=pathlib.Path, default=None)
    opcoes = argumentos.parse_args()

    with tempfile.TemporaryDirectory() as temporaria:
        pasta = pathlib.Path(temporaria) / "ui"
        shutil.copytree(opcoes.casca, pasta)
        pagina = pasta / "index.html"
        texto = pagina.read_text()
        corte = texto.index("<script src=")
        roteiro = ""
        if opcoes.roteiro:
            corpo = opcoes.roteiro.read_text()
            roteiro = CABECA_DO_ROTEIRO + corpo + PE_DO_ROTEIRO
        pagina.write_text(texto[:corte] + DUBLE + texto[corte:] + roteiro)

        saida = subprocess.run(
            [
                navegador(),
                "--headless",
                "--disable-gpu",
                f"--window-size={opcoes.janela}",
                "--virtual-time-budget=5000",
                "--screenshot=" + str(pasta / "foto.png"),
                f"file://{pagina}",
            ],
            capture_output=True,
            text=True,
        )

        if opcoes.foto:
            shutil.copy(pasta / "foto.png", opcoes.foto)

    tudo = saida.stdout + saida.stderr
    estouros = sorted(set(re.findall(r"Uncaught [A-Za-z]*Error: [^\"]*", tudo)))
    relatos = re.findall(r"SEELE-RELATO ([^\"]*)", tudo)

    for linha in relatos:
        for parte in linha.split(" | "):
            print(parte)
    if estouros:
        print("\nestouros na carga:")
        for e in estouros:
            print(f"  {e}")
        return 1
    print("\ncarga limpa: nenhum script estourou.")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
