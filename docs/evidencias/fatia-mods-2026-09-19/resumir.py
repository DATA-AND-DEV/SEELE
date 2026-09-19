"""Compara as fases medidas: memória e taxa de CPU."""
import json, sys
from pathlib import Path

aqui = Path(__file__).parent

def cpu_de(ps, pid):
    for linha in ps.strip().splitlines():
        campos = linha.split()
        if campos and campos[0] == str(pid):
            minutos, segundos = campos[2].split(":")
            return int(minutos) * 60 + float(segundos)
    return None

for fase in sys.argv[1:]:
    d = json.loads((aqui / f"{fase}.json").read_text())
    a, b = d[0], d[-1]
    dur = b["unix"] - a["unix"]
    pids = [p["pid"] for p in a["processos"]]
    print(f"== {fase} — janela de {dur:.1f} s ==")
    soma = 0.0
    for pid in pids:
        x, y = cpu_de(a["ps"], pid), cpu_de(b["ps"], pid)
        if x is None or y is None:
            continue
        soma += y - x
        nome = next(p["nome"] for p in a["processos"] if p["pid"] == pid)
        print(f"   {nome[:34]:34} cpu +{y-x:5.2f}s  ({100*(y-x)/dur:4.1f}% de um núcleo)")
    fp = sorted(r["total_footprint_bytes"] / 2**20 for r in d)
    print(f"   soma de CPU: {soma:.2f}s = {100*soma/dur:.1f}% de um núcleo")
    print(f"   footprint: mediana {fp[len(fp)//2]:.1f} MiB (min {fp[0]:.1f}, max {fp[-1]:.1f})")
