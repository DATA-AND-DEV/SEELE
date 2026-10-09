#!/usr/bin/env python3
"""M3 da 0.16 — o custo dos MODs na janela, medido na v0.15.1 publicada.

Um cenário por chamada. Cada um sobe um servidor novo (`m3-servidor`) com 0, 1
ou 3 MODs oficiais, abre o **app publicado, sem mudança nenhuma**, com uma
pasta de dados descartável (`SEELE_HOME`), entra pela linha de comando
(`--entrar`), conduz a janela pela acessibilidade (`ax`) e mede, por
`--segundos` (60 por padrão):

- **memória**: `phys_footprint` do `seele-app` e dos três processos do WebKit
  que nasceram com ele (WebContent, GPU, Networking), por `footprint`;
- **CPU**: o tempo de CPU somado desses quatro processos no intervalo, por `ps`;
- **tempo de quadro**: `xctrace` com o modelo «Animation Hitches» preso ao
  app — os quadros mostrados por segundo e os engasgos (hitches);
- **jitter da voz**: o número que a própria barra de telemetria mostra
  (JITTER), lido a cada 2 s pela acessibilidade, com o `m3-falante` falando
  na sala 1.

**Não perturba quem usa a máquina.** A janela precisa estar no palco (uma
janela encoberta tem o desenho estrangulado pelo WebKit), e trazê-la tira o
foco de quem estiver digitando. Por isso o cenário só começa com a máquina
ociosa há `--ocioso` segundos (300 por padrão), e é marcado INTERROMPIDO se
houver qualquer entrada durante ele. O arraste da MESA usa o tap de sessão,
que não conta como entrada no `HIDIdleTime`.

Uso:
    python3 m3.py <cenario> [--segundos 60] [--ocioso 300] [--saida DIR]

Cenários (ADR 0052:189-193, mais o 0/1/3 do plano da F3):
    sem-mod  1-mod  3-mods  3-mods-editor  3-mods-mesa
    e cada um com `+voz` (por exemplo `3-mods+voz`).
"""

import argparse
import json
import os
import pathlib
import re
import shutil
import statistics
import subprocess
import sys
import time

AQUI = pathlib.Path(__file__).resolve().parent
RAIZ = AQUI.parent  # scratchpad/m3
AX = RAIZ / "ax"
SERVIDOR = RAIZ / "alvo/release/examples/m3-servidor"
FALANTE = RAIZ / "alvo/release/examples/m3-falante"
APP = pathlib.Path("/Applications/SEELE.app")
PACOTES = {
    "perfis": RAIZ / "pacotes/seele-perfis-3.3.0",
    "mesa": RAIZ / "pacotes/seele-mesa-3.1.0",
    "estilo": RAIZ / "pacotes/seele-estilo-3.1.0",
}
PORTA = 8391  # nunca a 8383 (app) nem a 8384 (bateria de testes)
# Os quadros são gravados nos primeiros 30 s da janela de 60: o trace de 60 s
# com os instrumentos de engasgo passava de 500 MB e o xctrace levava mais de
# 10 min para salvá-lo, a 99 % de CPU, sujando o cenário seguinte.
JANELA_DE_QUADROS = 30


def sh(*args, **kw):
    return subprocess.run([str(a) for a in args], capture_output=True, text=True, **kw)


def ax(*args):
    r = sh(AX, *args)
    return r.stdout


def ocioso_s() -> float:
    saida = sh("ioreg", "-c", "IOHIDSystem").stdout
    m = re.search(r'"HIDIdleTime" = (\d+)', saida)
    return int(m.group(1)) / 1e9 if m else 0.0


def bloqueada() -> bool:
    """A sessão gráfica bloqueada: a janela não vai ao palco e a árvore de
    acessibilidade não responde, então nada se mede. O critério de máquina
    ociosa se cumpre justamente quando ela bloqueia (medido em 08/10)."""
    return "bloqueada=true" in sh(RAIZ / "sessao").stdout


def esperar(condicao, prazo, passo=0.5):
    fim = time.time() + prazo
    while time.time() < fim:
        if condicao():
            return True
        time.sleep(passo)
    return False


def tem(pid, texto):
    return texto.lower() in ax("textos", pid, texto).lower()


def irmaos_webkit(pid: int):
    """Os processos do WebKit que nasceram com este app: PIDs logo acima do
    dele, abertos até 15 s depois dele. O WebKit os abre por XPC, com o
    `launchd` como pai, então o parentesco não serve."""
    inicio = sh("ps", "-o", "lstart=", "-p", pid).stdout.strip()
    t0 = time.mktime(time.strptime(inicio, "%a %b %d %H:%M:%S %Y"))
    achados = {}
    for linha in sh("ps", "-axo", "pid=,lstart=,comm=").stdout.splitlines():
        m = re.match(r"\s*(\d+)\s+(\w+ \w+\s+\d+ [\d:]+ \d+)\s+(.*)", linha)
        if not m or "WebKit" not in m.group(3) or "Simulator" in m.group(3):
            continue
        outro, quando = int(m.group(1)), time.mktime(time.strptime(m.group(2), "%a %b %d %H:%M:%S %Y"))
        if outro > pid and 0 <= quando - t0 <= 15:
            nome = m.group(3).rsplit(".", 1)[-1]
            achados[nome] = outro
    return achados


def memoria(pids: dict) -> dict:
    args = []
    for p in pids.values():
        args += ["-p", str(p)]
    saida = sh("footprint", "-f", "bytes", "--noCategories", *args).stdout
    por_pid = {int(m.group(2)): int(m.group(3)) for m in
               re.finditer(r"^(\S+) \[(\d+)\]: 64-bit\s+Footprint: (\d+) B", saida, re.M)}
    return {nome: por_pid.get(p, 0) for nome, p in pids.items()}


def cpu_s(pids: dict) -> float:
    total = 0.0
    for p in pids.values():
        t = sh("ps", "-o", "cputime=", "-p", p).stdout.strip()
        if not t:
            continue
        partes = [float(x) for x in t.replace("-", ":").split(":")]
        while len(partes) < 3:
            partes.insert(0, 0.0)
        total += partes[-3] * 3600 + partes[-2] * 60 + partes[-1]
    return total


def jitter_ms(pid) -> float | None:
    """O valor que a barra de telemetria mostra ao lado de JITTER."""
    linhas = ax("arvore", pid, 45).splitlines()
    for i, linha in enumerate(linhas):
        if linha.strip().endswith("| JITTER"):
            for prox in linhas[i + 1:i + 4]:
                m = re.search(r"\|\s*([\d.]+)\s*ms\s*$", prox)
                if m:
                    return float(m.group(1))
    return None


QUADRO_MS = 1000 / 60  # o orçamento de um quadro a 60 Hz, o ritmo do WKWebView


def quadros(trace: pathlib.Path, pid: int, pasta: pathlib.Path, segundos: int) -> dict:
    """Os commits de Core Animation do app, lidos do trace (instrumento «Core
    Animation Commits»). No WebKit, quem faz o commit do quadro da página é o
    processo do app (o `seele-app`); o filtro é pelo PID no texto da thread,
    porque o trace tem a máquina inteira. Um commit acima do orçamento de um
    quadro é contado como engasgo."""
    sys.path.insert(0, str(AQUI))
    from xctrace_tabela import linhas
    r = sh("xcrun", "xctrace", "export", "--input", trace, "--xpath",
           '/trace-toc/run[@number="1"]/data/table[@schema="coreanimation-commit-interval"]')
    (pasta / "coreanimation-commit-interval.xml").write_text(r.stdout)
    todas = linhas(r.stdout) if r.stdout.strip() else []
    marca = f"pid: {pid})"
    commits = sorted(int(x["duration"][0]) / 1e6 for x in todas
                     if marca in (x.get("thread", (None, ""))[1] or ""))
    def q(p):
        return round(commits[min(len(commits) - 1, int(len(commits) * p))], 2) if commits else None
    acima = [c for c in commits if c > QUADRO_MS]
    return {
        "janela_s": segundos,
        "commits": len(commits),
        "commits_por_s": round(len(commits) / segundos, 1),
        "commit_ms": {"p50": q(0.5), "p95": q(0.95), "p99": q(0.99),
                       "max": round(commits[-1], 2) if commits else None},
        "acima_de_um_quadro": len(acima),
        "tempo_acima_ms": round(sum(c - QUADRO_MS for c in acima), 1),
    }


def cpu_da_maquina() -> float:
    """A CPU somada de todos os processos agora, em núcleos (100 % = 1)."""
    total = sum(float(x) for x in sh("ps", "-A", "-o", "pcpu=").stdout.split() if x.replace(".", "").isdigit())
    return round(total / 100, 2)


def maquina_quieta(prazo=1800):
    """Espera não haver compilação de outra sessão rodando: um `rustc` em
    paralelo suja CPU e tempo de quadro. A média de carga não serve de
    critério, porque decai devagar depois de uma compilação; a CPU somada de
    agora serve. Devolve a CPU somada (em núcleos) no começo, ou None."""
    fim = time.time() + prazo
    while time.time() < fim:
        compilando = sh("pgrep", "-x", "rustc").stdout.split()
        nucleos = cpu_da_maquina()
        if not compilando and nucleos < 2:
            return nucleos
        time.sleep(30)
    return None


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("cenario")
    ap.add_argument("--segundos", type=int, default=60)
    ap.add_argument("--ocioso", type=int, default=300)
    ap.add_argument("--saida", default=str(RAIZ / "medidas"))
    a = ap.parse_args()

    base, _, extra = a.cenario.partition("+")
    voz = extra == "voz"
    mods = {"sem-mod": [], "1-mod": ["perfis"]}.get(base, ["perfis", "mesa", "estilo"])
    relato = {"cenario": a.cenario, "mods": mods, "voz": voz, "segundos": a.segundos,
              "app": sh("defaults", "read", APP / "Contents/Info.plist", "CFBundleShortVersionString").stdout.strip()}

    carga = maquina_quieta()
    if carga is None:
        print(f"{a.cenario}: outra sessão compila há 30 min; não começo")
        sys.exit(4)
    relato["nucleos_ocupados_no_comeco"] = carga
    relato["carga_media_no_comeco"] = sh("sysctl", "-n", "vm.loadavg").stdout.strip()
    if ocioso_s() < a.ocioso:
        print(f"{a.cenario}: a máquina não está ociosa há {a.ocioso} s; não começo")
        sys.exit(3)
    if bloqueada():
        print(f"{a.cenario}: a sessão está bloqueada; não começo")
        sys.exit(3)
    pasta = pathlib.Path(a.saida) / f"{a.cenario}-{time.strftime('%H%M%S')}"
    pasta.mkdir(parents=True)

    servidor = falante = None
    pid = None
    comeco = time.time()
    _sintetico["ultimo"] = comeco
    mexeu = False

    def pessoa_mexeu():
        return ocioso_s() < (time.time() - _sintetico["ultimo"]) - 1.0
    try:
        mundo = pasta / "mundo"
        servidor = subprocess.Popen([str(SERVIDOR), str(PORTA), str(mundo), *[str(PACOTES[m]) for m in mods]],
                                    stdout=subprocess.PIPE, stderr=open(pasta / "servidor.err", "w"), text=True)
        linha = servidor.stdout.readline()
        fp = re.search(r"fp=([0-9a-f]+)", linha).group(1)
        casa = pasta / "casa"
        casa.mkdir()
        # O cache por hash poupa o indexador (e a variação da rede) em cada
        # cenário; sem ele, o app baixa do indexador, como baixaria para
        # qualquer pessoa. O aceite não é copiado: o app pede, e o roteiro
        # aperta ACEITAR, como uma pessoa faria.
        if mods and (RAIZ / "cache/mod-packages").is_dir():
            shutil.copytree(RAIZ / "cache/mod-packages", casa / "mod-packages")
        antes = set(int(x) for x in sh("pgrep", "-f", "SEELE.app/Contents/MacOS/seele-app").stdout.split())
        sh("open", "-n", "-a", APP, "--env", f"SEELE_HOME={casa}", "--args", "--entrar",
           f"seele://127.0.0.1:{PORTA}/?fp={fp}")
        def novo_pid():
            agora = set(int(x) for x in sh("pgrep", "-f", "SEELE.app/Contents/MacOS/seele-app").stdout.split())
            return (agora - antes) or None
        assert esperar(novo_pid, 20), "o app não abriu"
        pid = novo_pid().pop()
        relato["pid"] = pid
        assert esperar(lambda: tem(pid, "ENTRAR NO SERVIDOR") or tem(pid, "ACEITAR"), 40), "a entrada não apareceu"
        if tem(pid, "ACEITAR"):
            ax("apertar", pid, "ACEITAR")
            assert esperar(lambda: tem(pid, "ENTRAR NO SERVIDOR"), 40)
        ax("apertar", pid, "ENTRAR NO SERVIDOR")
        assert esperar(lambda: tem(pid, "SAIR DO SERVIDOR"), 40), "a sessão não abriu"
        ax("apertar", pid, "dispensar")
        ax("ativar", pid)
        time.sleep(3)
        if voz:
            falante = subprocess.Popen([str(FALANTE), f"127.0.0.1:{PORTA}", "1", str(a.segundos + 120)],
                                       stdout=open(pasta / "falante.out", "w"), stderr=subprocess.STDOUT)
            time.sleep(3)
            ax("apertar", pid, "ENTRAR NA SALA")
            assert esperar(lambda: tem(pid, "SAIR DA SALA"), 20), "não entrou na sala"
        acao = ACOES.get(base)
        if acao:
            relato["acao"] = acao(pid, pasta)
        if base == "3-mods-mesa":
            arrastar_na_mesa(pid)  # acha a peça antes da janela de medida
            relato["peca"] = _peca.get("xy")
        time.sleep(20)  # assentar: MODs desenhados, mídia carregada, voz estável
        ax("ativar", pid)
        # A prova de que os MODs estão vivos, e não só carregados: as entradas
        # que eles registram em `servidor.navegacao` («Perfis — de seele/perfis»).
        # Sem isto, um MOD que ativa e não desenha (como com o pacote instalado
        # por identificador) mediria igual a um que desenha.
        relato["entradas_dos_mods"] = sorted(set(
            l.split("|")[1].strip() for l in ax("textos", pid, "— de seele/").splitlines()
            if l.startswith("AXButton")))
        pids = {"seele-app": pid, **irmaos_webkit(pid)}
        relato["processos"] = pids
        trace = pasta / "quadros.trace"
        gravacao = subprocess.Popen(["xcrun", "xctrace", "record", "--instrument", "Core Animation Commits",
                                     "--attach", str(pid), "--time-limit", f"{JANELA_DE_QUADROS}s",
                                     "--output", str(trace), "--no-prompt"],
                                    stdout=open(pasta / "xctrace.out", "w"), stderr=subprocess.STDOUT)
        cpu0, t0 = cpu_s(pids), time.time()
        amostras_mem, jitters = [], []
        rustc_durante = 0  # compilação de outra sessão no meio da janela suja CPU e quadro
        while time.time() - t0 < a.segundos:
            if sh("pgrep", "-x", "rustc").stdout.strip():
                rustc_durante += 1
            if acao_continua := CONTINUAS.get(base):
                mexeu = mexeu or pessoa_mexeu()  # antes do próximo evento do roteiro
                acao_continua(pid)
            if voz:
                j = jitter_ms(pid)
                if j is not None:
                    jitters.append(j)
            if time.time() - t0 >= 15 * len(amostras_mem):  # 0, 15, 30, 45 s
                amostras_mem.append(memoria(pids))
            time.sleep(2)
        cpu1, t1 = cpu_s(pids), time.time()
        amostras_mem.append(memoria(pids))
        try:
            gravacao.wait(timeout=600)
        except subprocess.TimeoutExpired:
            gravacao.kill()  # um xctrace preso a 99 % de CPU suja o cenário seguinte
            raise
        # qualquer entrada desde o começo do cenário o invalida: a janela pode
        # ter perdido o palco, e o foco pode ter ido para outra coisa
        relato["bloqueou"] = bloqueada()
        relato["interrompido"] = mexeu or pessoa_mexeu() or relato["bloqueou"]
        relato["rustc_durante"] = rustc_durante
        relato["memoria_mib"] = {
            nome: round(statistics.median(m[nome] for m in amostras_mem) / 2**20, 1) for nome in pids}
        relato["memoria_mib"]["total"] = round(sum(relato["memoria_mib"].values()), 1)
        relato["cpu_pct"] = round(100 * (cpu1 - cpu0) / (t1 - t0), 1)
        relato["quadros"] = quadros(trace, pid, pasta, JANELA_DE_QUADROS)
        shutil.rmtree(trace, ignore_errors=True)  # centenas de MB; as tabelas ficam em XML
        relato["nucleos_ocupados_no_fim"] = cpu_da_maquina()
        relato["carga_media_no_fim"] = sh("sysctl", "-n", "vm.loadavg").stdout.strip()
        relato["rustc_no_fim"] = len(sh("pgrep", "-x", "rustc").stdout.split())
        if voz:
            relato["jitter_ms"] = {
                "amostras": len(jitters),
                "mediana": statistics.median(jitters) if jitters else None,
                "max": max(jitters) if jitters else None,
            }
    finally:
        if pid:
            sh("kill", "-TERM", pid)
        for proc in (falante, servidor):
            if proc:
                proc.terminate()
        (pasta / "relato.json").write_text(json.dumps(relato, indent=2, ensure_ascii=False))
        print(json.dumps(relato, ensure_ascii=False))


# ------------------------------------------------------------------ as ações
def abrir_editor_do_perfis(pid, pasta):
    """«Com um editor aberto» (ADR 0052:190): EDITAR MEU PERFIL, do PERFIS, que
    abre ao tocar no próprio cartão da lista (medido em 08/10 na 3.3.0)."""
    # O cartão só passa a dizer «— perfil em seele/perfis» depois de o PERFIS o
    # registrar, e ele consulta o servidor a cada 4 s: tocar antes abre o perfil
    # do produto, e não o editor do MOD.
    if not esperar(lambda: tem(pid, "(você) — perfil em"), 30):
        return "o cartão do PERFIS não apareceu"
    ax("apertar", pid, "(você) — perfil em")
    ok = esperar(lambda: tem(pid, "EDITAR MEU PERFIL"), 15)
    return "EDITAR MEU PERFIL aberto" if ok else "o editor não abriu"


def preparar_mesa(pid, pasta):
    """«MESA em arraste» (ADR 0052:191). O caminho pela janela, medido em 08/10
    na MESA 3.1.0: a entrada «Mesa — de seele/mesa», CRIAR MESA, CRIAR CENA, a
    cena posta na mesa pelo seletor, e quatro peças, que nascem na casa (0, 0).
    """
    # Cada passo espera o que o anterior faz aparecer: a entrada da MESA só
    # existe depois de o MOD registrá-la, e cada tela dela depois de o servidor
    # responder.
    passos = [
        ("Mesa — de seele/mesa", ("apertar", "Mesa — de seele/mesa")),
        ("NOME DA CAMPANHA", ("escrever", "NOME DA CAMPANHA", "Campanha M3")),
        ("CRIAR MESA", ("apertar", "CRIAR MESA")),
        ("CENA NOVA", ("escrever", "CENA NOVA", "Cena M3")),
        ("CRIAR CENA", ("apertar", "CRIAR CENA")),
        ("CENA EM CIMA DA MESA", ("apertar", "CENA EM CIMA DA MESA")),
        ("Cena M3", ("apertar", "Cena M3")),
        ("PÔR PEÇA", None),
    ]
    feito = []
    for espera, passo in passos:
        if not esperar(lambda: tem(pid, espera), 30):
            feito.append(f"não apareceu: {espera}")
            return feito
        if passo:
            feito.append(ax(passo[0], pid, *passo[1:]).strip())
            time.sleep(2)
    for n in range(4):
        ax("escrever", pid, "PEÇA NOVA", f"P{n + 1}")
        ax("apertar", pid, "PÔR PEÇA")
        time.sleep(2)
    ax("rolar", pid, "TABULEIRO")
    time.sleep(2)
    return feito


_peca = {}
# O arraste é um evento sintético, e ele zera o `HIDIdleTime` mesmo pelo tap de
# sessão (medido em 08/10). Por isso «uma pessoa mexeu» é o tempo ocioso menor
# que o tempo desde o último evento do próprio roteiro, e não desde o começo.
_sintetico = {"ultimo": None}


def arrastar_na_mesa(pid):
    """Um arraste de 1,5 s, ida e volta, saindo da peça. A peça é achada na
    captura da janela (o bloco laranja dela), entre o título da cena e o campo
    DADOS — o `<canvas>` não aparece na árvore de acessibilidade."""
    if "xy" not in _peca:
        estado = ax("estado", pid).split()
        wx, wy, ww = (int(v) for v in estado[1:4])
        caixas = []
        for l in ax("caixas", pid, 20, 3000).splitlines():
            partes = l.split()
            # fundura, papel, [subpapel,] x, y, largura, altura, descrição…
            numeros = [int(v) for v in partes[2:7] if v.lstrip("-").isdigit()][:4]
            if len(numeros) == 4:
                caixas.append((partes[1], *numeros))
        campos = sorted(c for c in caixas if c[0] == "AXTextField" and c[2] >= wy)
        campos.sort(key=lambda c: c[2])
        dados_y = campos[0][2]  # o primeiro campo da tela da mesa é DADOS
        # o tabuleiro fica entre o título da cena (o último antes de DADOS) e DADOS
        titulos = [c for c in caixas if c[0] == "AXHeading" and wy <= c[2] < dados_y]
        topo = max(c[2] + c[4] for c in titulos) if titulos else wy
        base = dados_y
        png = RAIZ / "peca.png"
        numero = ax("numero", pid).split()[0]
        sh("screencapture", "-x", "-o", "-l", numero, png)
        largura_png = int(sh("sips", "-g", "pixelWidth", png).stdout.split()[-1])
        r = ax("laranja", png, wx, wy, wx, topo, wx + ww - 330, base, largura_png / ww).split()
        _peca["xy"] = (int(r[0]), int(r[1])) if len(r) == 2 and r[0].isdigit() else None
    if not _peca["xy"]:
        return
    x, y = _peca["xy"]
    sh(AX, "arrastar", x, y, x + 240, y + 120, 1500)
    _sintetico["ultimo"] = time.time()


ACOES = {"3-mods-editor": abrir_editor_do_perfis, "3-mods-mesa": preparar_mesa}
CONTINUAS = {"3-mods-mesa": arrastar_na_mesa}

if __name__ == "__main__":
    main()
