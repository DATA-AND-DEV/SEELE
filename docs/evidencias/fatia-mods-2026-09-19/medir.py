"""Consumo do aplicativo numa fase, medido e não estimado.

Mesma forma da medição de 18/09: `footprint` para a memória real da família de
processos e `ps` para RSS e tempo de CPU acumulado. Sete amostras a cada cinco
segundos, porque uma amostra só não distingue estado de transiente.

O tempo de CPU é **acumulado desde o início do processo**, então o que importa
é a diferença entre a primeira e a última amostra da fase — a taxa. Um total
grande com taxa zero é um processo que trabalhou na subida e está parado.
"""
import json, re, subprocess, sys, time
from pathlib import Path

fase = sys.argv[1]
pids = sys.argv[2:]
saida = Path(__file__).parent
linhas = []
for i in range(7):
    inicio = time.time()
    r = subprocess.run(
        ["/usr/bin/footprint", *sum((["-p", p] for p in pids), []), "--noCategories", "-f", "bytes"],
        capture_output=True, text=True,
    )
    amostras = [
        dict(nome=m[1], pid=int(m[2]), footprint_bytes=int(m[3]))
        for m in re.finditer(r"^(.+?) \[(\d+)\]:.*?Footprint: (\d+) B", r.stdout, re.M)
    ]
    ps = subprocess.run(
        ["ps", "-p", ",".join(pids), "-o", "pid=,rss=,time=,comm="],
        capture_output=True, text=True,
    ).stdout
    total = re.search(r"Summary Footprint: (\d+) B", r.stdout)
    linhas.append(dict(
        fase=fase, indice=i, unix=inicio, processos=amostras,
        total_footprint_bytes=int(total[1]) if total else None, ps=ps,
        returncode=r.returncode,
    ))
    (saida / f"{fase}.json").write_text(json.dumps(linhas, indent=2))
    print(fase, i, round(linhas[-1]["total_footprint_bytes"] / 2**20, 2) if total else "ERRO", flush=True)
    if i < 6:
        time.sleep(max(0, 5 - (time.time() - inicio)))
