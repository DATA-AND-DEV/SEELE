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
const SEELE_RETRATO = "data:image/png;base64,iVBORw0KGgoAAAANSUhEUgAAAAgAAAAICAIAAABLbSncAAAAEUlEQVR4nGP4FCSPFTEMLQkA4oZYwU22lhgAAAAASUVORK5CYII=";
const SEELE_RESPOSTAS = {
  hospedar: { aqui: "127.0.0.1:8383", convite: "seele://127.0.0.1:8383?fp=abc",
              alcance: "SoRedeLocal", porta_recusada: null, encontro_recusado: null },
};
// A mesma massa que a comp desenha, para as duas telas poderem ser postas lado
// a lado. Nomes de campo do `Snapshot` de `seele-ffi`, e não inventados: um
// campo com nome errado desenha um buraco, e o buraco parece defeito da casca.
const P = (id, nome, opcoes = {}) => ({
  id, nickname: nome, speaking: false, muted: false, total_isolation: false,
  signal: 96, sync_band: "Verde", is_self: false, ...opcoes,
});
const SEELE_QUADRO = {
  caminho: "UPNP", link: "Up", link_state: "Verified", server: "TÓQUIO-3",
  icon_revision: 0, me: 1, nickname: "rafa",
  voice_rooms: [
    { id: 1, name: "PONTE", limit: 8, password_required: false, occupied_by_us: true,
      channel: null, sync: null,
      people: [P(1, "rafa", { is_self: true }), P(2, "juli", { speaking: true, signal: 88 }),
               P(3, "vitor", { signal: 71, sync_band: "Laranja" })] },
    { id: 2, name: "OFICINA", limit: 6, password_required: false, occupied_by_us: false,
      channel: null, sync: null, people: [P(4, "dani", { signal: 54, sync_band: "Laranja" })] },
    { id: 3, name: "SILÊNCIO", limit: 4, password_required: false, occupied_by_us: false,
      channel: null, sync: null, people: [] },
  ],
  presentes: [P(1, "rafa", { is_self: true }), P(2, "juli"), P(3, "vitor"), P(4, "dani")],
  channels: [
    { id: 1, name: "geral", open: true }, { id: 2, name: "obras", open: false },
    { id: 3, name: "links", open: false }, { id: 4, name: "ruido", open: false },
  ],
  messages_revision: 1,
  telemetry: { rtt_ms: 23, jitter_ms: 4, loss_fraction: 0.002, bitrate_bps: 48000,
               signal: 92, sync_band: "Verde", input_level: 0.4, local_fault: false,
               frames_refused: 0 },
  notice: null, muted: false, total_isolation: false, speaking: false,
  person_icons_revision: 1,
  voice_mode: "PushToTalk", audio_available: true, capture: null, playback: null,
  may_manage_voice_rooms: true, may_kick: true, may_ban: true, may_remove_message: true,
  may_move_person: true, may_customise_server: true, may_delete_rooms: true,
  tela: null, ended: null,
};
const M = (id, autor, nome, quando, corpo, propria = false) => ({
  id, channel: 1, author: autor, author_nickname: nome, at_seconds: quando,
  body: corpo, own: propria, edited: false, attachment: null,
});
SEELE_RESPOSTAS.messages = [
  M(1, 2, "juli", 1756000442, "subi o seeled aqui de casa, a porta 8383 tá aberta no roteador"),
  M(2, 3, "vitor", 1756000480, "UDP? levei meia hora pra descobrir que a regra que eu tinha escrito era TCP"),
  M(3, 1, "rafa", 1756000511, "UDP. tá escrito no README, duas linhas depois do comando", true),
  M(4, 2, "juli", 1756000563, "o furo de NAT funcionou daqui pro celular do vitor, o encontro só apresentou os dois e esqueceu"),
  M(5, 4, "dani", 1756000649, "entrei na oficina, alguém vem?"),
];
SEELE_RESPOSTAS.snapshot = SEELE_QUADRO;
SEELE_RESPOSTAS.connect = { snapshot: SEELE_QUADRO, veredito: null };
window.__SEELE_CHAMADAS = [];
window.__SEELE_ARGS = {};
window.__SEELE_EM_SESSAO = false;
window.__SEELE_APELIDO = "";
window.__SEELE_OUVINTES = {};
window.__SEELE_EMITIR = (carga) => {
  for (const ouvinte of window.__SEELE_OUVINTES["seele://event"] ?? []) {
    ouvinte({ payload: carga });
  }
};
window.__TAURI__ = {
  core: {
    invoke: async (cmd, args) => {
      window.__SEELE_CHAMADAS.push(cmd);
      // Os argumentos de cada comando, para um roteiro poder afirmar sobre o
      // que a casca **mandou** e não só sobre o que ela chamou. No duble e não
      // num embrulho de fora: os módulos capturam o `invoke` na carga, e trocá-lo
      // depois não alcança ninguém.
      window.__SEELE_ARGS[cmd] = args;
      // **Sem sessão, `snapshot` recusa** — como o produto faz. Responder um
      // quadro na tela inicial faria o duble mentir sobre a única pergunta que
      // separa os dois modos do diálogo de perfil, e um aparelho que mente
      // sobre isso não prova nada sobre ele.
      if (cmd === "connect" || cmd === "hospedar") window.__SEELE_EM_SESSAO = true;
      if (cmd === "disconnect") window.__SEELE_EM_SESSAO = false;
      if (cmd === "snapshot" && !window.__SEELE_EM_SESSAO) throw "NotConnected";
      if (cmd in SEELE_RESPOSTAS) return SEELE_RESPOSTAS[cmd];
      if (/conhecid|fontes|lista|dispositiv|visitad|salas|canais|pessoas/i.test(cmd)) return [];
      // Um retrato para uma pessoa só: o que se quer ver é a diferença entre um
      // avatar com imagem e um com iniciais, lado a lado no mesmo quadro.
      if (cmd === "imagem_da_pessoa") return args && args.person === 2 ? SEELE_RETRATO : null;
      // O retrato desta máquina, que é o que faz os dois diálogos de perfil
      // terem o mesmo conteúdo com sessão e sem.
      if (cmd === "meu_retrato") return SEELE_RETRATO;
      // Os bytes do retrato do servidor, que o roteiro pode trocar para provar
      // que a tela acompanha uma mudança.
      if (cmd === "icone_do_server") return window.__SEELE_ICONE_DO_SERVER ?? null;
      // O apelido desta máquina, como as preferências o guardam. O roteiro
      // pode trocá-lo por `window.__SEELE_APELIDO`.
      if (cmd === "apelido_local") return window.__SEELE_APELIDO ?? "";
      if (cmd === "escolher_apelido_local") {
        window.__SEELE_APELIDO = args && args.apelido;
        return null;
      }
      if (/apelido|nickname|preferenc|link|caminho/i.test(cmd)) return "";
      return null;
    },
    convertFileSrc: (p) => p,
  },
  // Os ouvintes ficam guardados: um roteiro pode emitir eventos do produto —
  // `ScreenOpened`, `ScreenFrame` — e testar a metade da casca que só existe
  // em resposta a eles.
  event: {
    listen: async (nome, ouvinte) => {
      (window.__SEELE_OUVINTES[nome] ||= []).push(ouvinte);
      return () => {};
    },
    emit: async () => {},
  },
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
    # Todo `console.*` da página, e não só os estouros: quando a casca falha em
    # silêncio, o `console.warn` é o que ela tinha para dizer.
    argumentos.add_argument("--tudo", action="store_true")
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

    if opcoes.tudo:
        for linha in tudo.splitlines():
            if "CONSOLE" in linha:
                print(linha.split("CONSOLE:", 1)[-1].strip())
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
