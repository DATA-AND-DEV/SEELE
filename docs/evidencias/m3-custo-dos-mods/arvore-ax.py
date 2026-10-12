#!/usr/bin/env python3
"""A árvore de acessibilidade assenta com N pessoas na lista? (pendência 63, (b))

    python3 -I arvore-ax.py <pasta-da-campanha> <N> [<N> ...] [--app SEELE.app] [--saida DIR]

**Por que existe.** Na rodada de 11/10, a `lista-251` reprovou antes de medir: a
página desenhava, e a acessibilidade não achava os nomes da lista nem, às
vezes, o ENTRAR NA SALA. Com a lista refeita inteira a cada quadro, os nós são
trocados a cada quadro, e quem usa leitor de tela vê a lista sumir e voltar. A
orquestradora decidiu que isso é da 0.16 se não assentasse com 50 pessoas; esta
sonda é a medida, e é o guarda do conserto.

**O que ela faz**, para cada N, com a pasta de uma campanha da API 6
(`preparar-api6.sh`, e o conserto do `preparar-conserto-63.sh` se for o caso):
o servidor da medição na 8393 (nunca a 8384 da bateria, nem a 8391 e a 8392 da
campanha), N-1 calados sem MOD (`m3-cartao-NNN`, o mesmo apelido da
`carga-cartoes`), o app da pasta numa casa descartável, a mesma entrada do
`m3.py`, 20 s para assentar, e então, por 120 s, a cada ~3 s: quantos nomes da
lista a acessibilidade acha (fundura 200), se o ENTRAR NA SALA está na árvore,
quanto a consulta levou e a CPU do WebContent. No fim, aperta o ENTRAR NA SALA
e espera o SAIR DA SALA.

**Assenta** quer dizer: em toda consulta, a lista inteira e o ENTRAR NA SALA na
árvore, e o aperto entrando na sala. A saída é 0 só se toda N pedida assentar;
1 se alguma não assentar; 2 se alguma rodada não valer (a sessão bloqueada no
meio: com ela bloqueada a janela não vai ao palco e a árvore não responde, e a
primeira medida de 11/10 com 10 pessoas foi assim).

**Medido em 11/10**, na build `6e845e0d+6717dc2c` (o redesenho juntado em um por
quadro, e a lista ainda refeita inteira):

| pessoas | a lista inteira | nomes achados | ENTRAR NA SALA | entrou | CPU do WebKit |
|---|---|---|---|---|---|
| 10 | 39 de 39 consultas | sempre 9 | 39 de 39 | sim | 6% |
| 50 | 18 de 38 | de 0 a 49, oscilando os 2 min | 37 de 38 | sim | 31% |
| 100 | 0 de 38 | de 0 a 49 | 37 de 38 | sim | 62% |

E de novo, já por este arquivo, que é o guarda (saída 1 nas duas): com 10, 38 de
39 — **numa consulta a lista inteira sumiu (0 nomes)**, porque ela é trocada de
uma vez; com 50, 10 de 38. Com 10 a perda é rara, mas existe: o critério é
toda consulta, e não a maioria. O conserto (a lista reconciliada pelo id) passa
com 10, 50 e 100.

**O prazo das consultas.** Toda consulta voltou em 0,2 s ou menos, até com 100
pessoas, e por isso o `ax` não ganhou um `AXUIElementSetMessagingTimeout`: o que
some da árvore some, e não demora. O tempo de cada consulta fica no
`linhas.json` de cada rodada; se ele passar de alguns segundos, é hora de
ganhá-lo.

**O que ela não mede**: o que um VoiceOver de verdade faz com uma árvore que
muda — ela mede se a árvore tem a lista quando é consultada.

Lê o roteiro da campanha (`<pasta>/roteiro/m3.py`) sem escrever nada na pasta
dela fora de `--saida` (por padrão, `<pasta>/arvore-ax`).
"""
import argparse
import json
import pathlib
import re
import subprocess
import sys
import time

PORTA = 8393
JANELA_S = 120


def principal():
    ap = argparse.ArgumentParser()
    ap.add_argument("pasta")
    ap.add_argument("pessoas", type=int, nargs="+")
    ap.add_argument("--app", default=None)
    ap.add_argument("--saida", default=None)
    a = ap.parse_args()
    pasta = pathlib.Path(a.pasta).resolve()
    # O roteiro da campanha, de onde vêm o servidor, o falante, o `ax` e a
    # entrada: o mesmo que mede, sem cópia que divirja. Sem bytecode na pasta.
    sys.dont_write_bytecode = True
    sys.path.insert(0, str(pasta / "roteiro"))
    import m3  # noqa: PLC0415
    app = pathlib.Path(a.app).resolve() if a.app else pasta / "alvo/release/bundle/macos/SEELE.app"
    saida = pathlib.Path(a.saida).resolve() if a.saida else pasta / "arvore-ax"
    resumos = [medir(m3, app, saida, n) for n in a.pessoas]
    if not all(r["vale"] for r in resumos):
        return 2
    return 0 if all(r["assenta"] for r in resumos) else 1


def cpu_do_webkit(m3, pids):
    saida = m3.sh("ps", "-o", "%cpu=", "-p", ",".join(str(p) for p in pids)).stdout.split()
    return round(sum(float(x) for x in saida), 1)


def medir(m3, app, saida, n):
    if m3.bloqueada():
        print("a sessão está bloqueada; não começo", flush=True)
        sys.exit(2)
    pasta = saida / f"{n}-{time.strftime('%H%M%S')}"
    pasta.mkdir(parents=True)
    servidor = falante = None
    pid = None
    linhas = []
    try:
        servidor = subprocess.Popen([str(m3.SERVIDOR), str(PORTA), str(pasta / "mundo")],
                                    stdout=subprocess.PIPE, stderr=open(pasta / "servidor.err", "w"), text=True)
        fp = re.search(r"fp=([0-9a-f]+)", servidor.stdout.readline()).group(1)
        calados = pasta / "calados.out"
        falante = subprocess.Popen([str(m3.FALANTE), f"127.0.0.1:{PORTA}", "0", "1200", "--calados", str(n - 1),
                                    "--apelido", "m3-cartao"], stdout=open(calados, "w"), stderr=subprocess.STDOUT)
        assert m3.esperar(lambda: "M3 calados prontos" in calados.read_text(), 600, passo=2), "os calados não entraram"
        prontos = int(re.search(r"prontos=(\d+)", calados.read_text()).group(1))
        time.sleep(10)  # a portaria repõe os apertos do app
        casa = pasta / "casa"
        casa.mkdir()
        processo = f"{app}/Contents/MacOS/seele-app"
        antes = set(int(x) for x in m3.sh("pgrep", "-f", processo).stdout.split())
        m3.sh("open", "-n", "-a", app, "--env", f"SEELE_HOME={casa}", "--args", "--entrar",
              f"seele://127.0.0.1:{PORTA}/?fp={fp}")

        def novo():
            return (set(int(x) for x in m3.sh("pgrep", "-f", processo).stdout.split()) - antes) or None
        assert m3.esperar(novo, 20), "o app não abriu"
        pid = novo().pop()
        tem = m3.tem
        assert m3.esperar(lambda: tem(pid, "ENTRAR NO SERVIDOR") or tem(pid, "ACEITAR")
                          or tem(pid, "COMO OS OUTROS VÃO TE CHAMAR"), 40), "a entrada não apareceu"
        if tem(pid, "COMO OS OUTROS VÃO TE CHAMAR"):
            m3.ax("apertar", pid, "CANCELAR")
            assert m3.esperar(lambda: tem(pid, "ENTRAR NO SERVIDOR") or tem(pid, "ACEITAR"), 40)
        if tem(pid, "ACEITAR"):
            m3.ax("apertar", pid, "ACEITAR")
            assert m3.esperar(lambda: tem(pid, "ENTRAR NO SERVIDOR"), 40)
        m3.ax("apertar", pid, "ENTRAR NO SERVIDOR")
        assert m3.esperar(lambda: tem(pid, "SAIR DO SERVIDOR"), 40), "a sessão não abriu"
        m3.ax("apertar", pid, "dispensar")
        m3.ax("ativar", pid)
        time.sleep(20)
        webkit = list(m3.irmaos_webkit(pid).values())
        inicio = time.time()
        while time.time() - inicio < JANELA_S:
            t0 = time.time()
            nomes = set(re.findall(r"\bm3-cartao-\d{3}\b", m3.ax("textos", pid, "m3-cartao-", "200")))
            t1 = time.time()
            entrar = "entrar na sala" in m3.ax("textos", pid, "ENTRAR NA SALA", "200").lower()
            t2 = time.time()
            linhas.append({"t": round(t0 - inicio, 1), "nomes": len(nomes), "nomes_s": round(t1 - t0, 1),
                           "entrar": entrar, "entrar_s": round(t2 - t1, 1),
                           "cpu_webkit": cpu_do_webkit(m3, webkit), "bloqueada": m3.bloqueada()})
            time.sleep(3)
        apertou = m3.ax("apertar", pid, "ENTRAR NA SALA").strip()
        entrou = m3.esperar(lambda: tem(pid, "SAIR DA SALA"), 30)
        inteira = sum(1 for linha in linhas if linha["nomes"] >= prontos)
        com_entrar = sum(1 for linha in linhas if linha["entrar"])
        resumo = {
            "pessoas": n, "calados_prontos": prontos, "consultas": len(linhas),
            "consultas_com_a_lista_inteira": inteira,
            "nomes_achados": sorted({linha["nomes"] for linha in linhas}),
            "entrar_na_arvore": com_entrar,
            "consulta_s_max": max(max(linha["nomes_s"], linha["entrar_s"]) for linha in linhas),
            "cpu_webkit_mediana": sorted(linha["cpu_webkit"] for linha in linhas)[len(linhas) // 2],
            "apertou": apertou, "entrou_na_sala": entrou,
            "vale": not any(linha["bloqueada"] for linha in linhas) and not m3.bloqueada(),
        }
        resumo["assenta"] = (prontos == n - 1 and inteira == len(linhas) and com_entrar == len(linhas)
                             and entrou)
        (pasta / "linhas.json").write_text(json.dumps(linhas, indent=1))
        (pasta / "resumo.json").write_text(json.dumps(resumo, indent=1, ensure_ascii=False))
        print(json.dumps(resumo, ensure_ascii=False), flush=True)
        return resumo
    finally:
        if pid:
            m3.sh("kill", "-TERM", pid)
        for proc in (falante, servidor):
            if proc:
                proc.terminate()
        time.sleep(3)


if __name__ == "__main__":
    sys.exit(principal())
