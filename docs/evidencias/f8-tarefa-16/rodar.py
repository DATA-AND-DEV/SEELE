"""Tarefa 16 da F8: o desktop de verdade (cargo tauri dev), com e sem os arquivos
das cascas do navegador, fotografado nas mesmas telas.

Uso: python3 rodar.py <antes|depois|pontos|revertido>   (da raiz do worktree)

Isolado: SEELE_HOME numa pasta nova, a identidade numa pasta só desta prova,
SEELE_ENCONTRO=nao, o atualizador sem endereço (sem GitHub no conferir), e um
seeled de cópia em 127.0.0.1:8391. Toda conexão do processo do app é anotada
pelo lsof; o app e o servidor fecham no fim, dê certo ou não.
"""
import json, os, pathlib, shutil, subprocess, sys, threading, time

AQUI = pathlib.Path(__file__).resolve().parent
RAIZ = pathlib.Path.cwd()
AX = AQUI / "ax"
FP = "c911cf29922c37a5646e4e9efcade4984be3fe675af060d5c9d6a671fcfa0207"
LINK = f"seele://127.0.0.1:8391/?fp={FP}"
qual = sys.argv[1]
assert qual in ("antes", "depois", "revertido", "pontos")
saida = AQUI / f"fotos-{qual}"
shutil.rmtree(saida, ignore_errors=True); saida.mkdir()


def sh(*a, **k):
    return subprocess.run([str(x) for x in a], capture_output=True, text=True, **k)


def esperar(f, segundos, passo=0.5):
    fim = time.time() + segundos
    while time.time() < fim:
        r = f()
        if r:
            return r
        time.sleep(passo)
    return None


# O servidor: uma cópia nova do mesmo banco (o mesmo certificado, o mesmo estado).
srv = AQUI / f"srv-{qual}"
shutil.rmtree(srv, ignore_errors=True)
shutil.copytree(AQUI / "srv0", srv)
servidor = subprocess.Popen([RAIZ / "target/debug/seeled", "127.0.0.1:8391"], cwd=srv,
                            env={**os.environ, "SEELE_DB": str(srv / "seele.db")},
                            stdout=open(srv / "saida.txt", "w"), stderr=subprocess.STDOUT)

perfil = AQUI / f"perfil-{qual}"
shutil.rmtree(perfil, ignore_errors=True); perfil.mkdir()
(AQUI / "identidade").mkdir(exist_ok=True)
conf = AQUI / f"conf-{qual}.json"
conf.write_text(json.dumps({"build": {"frontendDist": str(AQUI / qual)},
                            "plugins": {"updater": {"endpoints": []}}}))
env = {**os.environ, "SEELE_HOME": str(perfil), "SEELE_IDENTIDADE": str(AQUI / "identidade"),
       "SEELE_ENCONTRO": "nao"}
antes_pids = set(sh("pgrep", "-f", "target/debug/seele-app").stdout.split())
dev = subprocess.Popen(["cargo", "tauri", "dev", "--no-watch", "--config", str(conf), "--", "--", "--entrar", LINK],
                       cwd=RAIZ / "apps/seele-app", env=env,
                       stdout=open(AQUI / f"dev-{qual}.log", "w"), stderr=subprocess.STDOUT)

rede, parar = set(), threading.Event()
relato = {"qual": qual, "fotos": [], "falhas": []}
pid = None
try:
    def novo_pid():
        agora = set(sh("pgrep", "-f", "target/debug/seele-app").stdout.split()) - antes_pids
        return agora.pop() if agora else None
    pid = esperar(novo_pid, 900, 1)
    assert pid, "o app não abriu em 15 minutos (veja dev-*.log)"
    relato["pid"] = pid

    def anotar():
        while not parar.is_set():
            for linha in sh("lsof", "-nP", "-i", "-a", "-p", pid).stdout.splitlines()[1:]:
                rede.add(" ".join(linha.split()[7:]))
            time.sleep(0.5)
    threading.Thread(target=anotar, daemon=True).start()

    def tem(texto):
        return sh(AX, "textos", pid, texto).stdout.strip()

    def tamanho(png):
        from PIL import Image
        return Image.open(png).size

    def parada(a, b):
        # Uma janela entrando no palco muda quase todo pixel; o relógio, o
        # UPTIME e o cursor de um campo mudam uns poucos. Parada é menos de
        # 0,5 % dos pixels diferentes entre as duas fotos.
        from PIL import Image, ImageChops
        ia, ib = Image.open(a).convert("L"), Image.open(b).convert("L")
        if ia.size != ib.size:
            return False
        diferentes = sum(ImageChops.difference(ia, ib).point(lambda v: 255 if v > 8 else 0).histogram()[255:])
        return diferentes < 0.005 * ia.size[0] * ia.size[1]

    def foto(nome):
        # Com o Stage Manager ligado, a janela de um binário sem pacote fica na
        # faixa do lado, e o `screencapture -l` fotografa a miniatura inclinada.
        # Traz ao palco pelos dois caminhos, e só aceita a foto do tamanho da
        # janela (1280 pontos, 2560 pixels na tela retina).
        png = saida / f"{nome}.png"
        for tentativa in range(6):
            sh("osascript", "-e", f'tell application "System Events" to set frontmost of (first process whose unix id is {pid}) to true')
            sh(AX, "ativar", pid)
            time.sleep(2.5)
            numero = sh(AX, "numero", pid).stdout.split()[0]
            sh("screencapture", "-x", "-o", "-l", numero, png)
            # Do tamanho exato da janela (1280 × 800 pontos), e parada: uma
            # segunda foto um segundo depois tem de dar os mesmos bytes de
            # imagem, fora o relógio. Medido: a primeira rodada pegou a janela
            # inclinada, no meio da entrada no palco.
            if not png.exists() or tamanho(png) != (2560, 1600):
                continue
            time.sleep(1.2)
            segunda = saida / f"{nome}-b.png"
            sh("screencapture", "-x", "-o", "-l", numero, segunda)
            if parada(png, segunda):
                segunda.unlink()
                relato["fotos"].append(nome)
                return
        relato["falhas"].append(f"{nome}: a janela não parou no palco; a foto tem {tamanho(png) if png.exists() else None}")

    inicio = time.time()
    # Com a identidade já criada e o perfil novo, o app pergunta o apelido antes
    # (medido: «COMO OS OUTROS VÃO TE CHAMAR?»). As duas rodadas usam a mesma
    # identidade, então as duas passam por ela: é mais uma tela a comparar.
    achou = esperar(lambda: tem("COMO OS OUTROS VÃO TE CHAMAR") or tem("ENTRAR NO SERVIDOR") or tem("ACEITAR"), 60)
    if achou and tem("COMO OS OUTROS VÃO TE CHAMAR"):
        foto("0-apelido")
        sh(AX, "apertar", pid, "FECHAR")
        time.sleep(1)
        achou = esperar(lambda: not tem("COMO OS OUTROS VÃO TE CHAMAR") and (tem("ENTRAR NO SERVIDOR") or tem("ACEITAR")), 30)
    relato["entrada_em_s"] = round(time.time() - inicio, 1) if achou else None
    assert achou, "a entrada não apareceu"
    foto("1-entrada")
    sh(AX, "apertar", pid, "ENTRAR NO SERVIDOR")
    assert esperar(lambda: tem("SAIR DO SERVIDOR"), 60), "a sessão não abriu"
    sh(AX, "apertar", pid, "dispensar")
    foto("2-sessao")
    if sh(AX, "apertar", pid, "CONFIGURAÇÕES").returncode == 0 or sh(AX, "apertar", pid, "Configurações").returncode == 0:
        esperar(lambda: tem("ÁUDIO"), 10)
        foto("3-ajustes")
    else:
        relato["falhas"].append("não achei CONFIGURAÇÕES")
except AssertionError as erro:
    relato["falhas"].append(str(erro))
    # O que a janela mostrava quando desistiu: a árvore e a foto, para medir em
    # vez de adivinhar.
    if pid:
        (saida / "falha-arvore.txt").write_text(sh(AX, "arvore", pid, 30).stdout)
        sh("osascript", "-e", f'tell application "System Events" to set frontmost of (first process whose unix id is {pid}) to true')
        sh(AX, "ativar", pid)
        time.sleep(3)
        numero = sh(AX, "numero", pid).stdout.split()
        if numero:
            sh("screencapture", "-x", "-o", "-l", numero[0], saida / "falha.png")
finally:
    parar.set()
    time.sleep(0.6)
    if pid:
        sh("kill", pid)
    dev.terminate()
    try:
        dev.wait(20)
    except subprocess.TimeoutExpired:
        dev.kill()
    servidor.terminate()
    servidor.wait(10)
    relato["rede"] = sorted(rede)
    relato["vivos"] = sh("pgrep", "-fl", "target/debug/seele-app|seeled 127.0.0.1:8391").stdout.strip()
    (AQUI / f"relato-{qual}.json").write_text(json.dumps(relato, ensure_ascii=False, indent=2))
    print(json.dumps(relato, ensure_ascii=False, indent=2))
