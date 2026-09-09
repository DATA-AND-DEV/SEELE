#!/usr/bin/env python3
"""Confere a cópia durável dos relatórios contra o inventário ao lado.

A cópia veio de um worktree em `/private/tmp`, que some com o reboot. O
inventário existe para que a conferência não dependa mais da origem: ele traz
sha256 e tamanho de cada arquivo, e este script relê a cópia e compara.

    python3 docs/superpowers/sdd/2026-09-09-integracao-malha-e-mods/conferir-inventario.py

Sai 0 se tudo bate, 1 se algum arquivo divergiu ou sumiu, 2 se sobrou arquivo
que o inventário não conhece.
"""

import hashlib
import os
import re
import sys

AQUI = os.path.dirname(os.path.abspath(__file__))
INVENTARIO = os.path.join(AQUI, "inventario-e-hashes.md")
COPIA = os.path.normpath(os.path.join(AQUI, "..", "2026-09-06-caminho-entre-pares"))

# `| <sha256> | <bytes> | <nome> |`
LINHA = re.compile(r"^\|\s*`([0-9a-f]{64})`\s*\|\s*([0-9]+)\s*\|\s*`([^`]+)`\s*\|")


def sha256(caminho):
    h = hashlib.sha256()
    with open(caminho, "rb") as f:
        for bloco in iter(lambda: f.read(1 << 20), b""):
            h.update(bloco)
    return h.hexdigest()


def main():
    # Duas tabelas no inventário, e elas moram em pastas diferentes: a de cima
    # são os 50 relatórios, a de baixo — depois do `## Fora da tabela` — é o
    # patch do trabalho não commitado, que fica numa subpasta. Ler as duas como
    # se fossem uma procuraria o patch entre os relatórios e não o acharia.
    esperado = {}
    patch = {}
    onde = esperado
    with open(INVENTARIO, encoding="utf-8") as f:
        for linha in f:
            if linha.startswith("## Fora da tabela"):
                onde = patch
            achou = LINHA.match(linha)
            if achou:
                onde[achou.group(3)] = (achou.group(1), int(achou.group(2)))
    if not esperado:
        print("o inventário não tem nenhuma linha legível", file=sys.stderr)
        return 1

    problemas = []
    for nome, (h_esperado, tam_esperado) in sorted(esperado.items()):
        caminho = os.path.join(COPIA, nome)
        if not os.path.isfile(caminho):
            problemas.append(f"SUMIU    {nome}")
            continue
        tam = os.path.getsize(caminho)
        h = sha256(caminho)
        if h != h_esperado:
            problemas.append(f"DIVERGE  {nome}  (sha256 {h[:12]}… ≠ {h_esperado[:12]}…)")
        elif tam != tam_esperado:
            problemas.append(f"TAMANHO  {nome}  ({tam} ≠ {tam_esperado})")

    for nome, (h_esperado, tam_esperado) in sorted(patch.items()):
        caminho = os.path.join(COPIA, "nao-commitado-em-wt2", nome)
        if not os.path.isfile(caminho):
            problemas.append(f"SUMIU    nao-commitado-em-wt2/{nome}")
        elif sha256(caminho) != h_esperado:
            problemas.append(f"DIVERGE  nao-commitado-em-wt2/{nome}")
        elif os.path.getsize(caminho) != tam_esperado:
            problemas.append(f"TAMANHO  nao-commitado-em-wt2/{nome}")

    presentes = {
        n for n in os.listdir(COPIA) if os.path.isfile(os.path.join(COPIA, n))
    }
    sobrando = sorted(presentes - set(esperado))

    print(
        f"{len(esperado)} relatórios e {len(patch)} patch no inventário, "
        f"{len(presentes)} arquivos na cópia"
    )
    for p in problemas:
        print(p)
    for s in sobrando:
        print(f"SOBRANDO {s}  (não está no inventário)")

    if problemas:
        return 1
    if sobrando:
        return 2
    print("tudo bate: a cópia é a mesma que foi conferida em 2026-09-09")
    return 0


if __name__ == "__main__":
    sys.exit(main())
