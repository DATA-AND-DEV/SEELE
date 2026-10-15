#!/usr/bin/env python3
"""M3 da 0.16 — a tabela da campanha da API 6, contra o `sem-mod` dela.

    python3 -I resumir-api6.py medidas-api6.json

Cada cenário é lido contra o `sem-mod` (ou o `sem-mod+voz`, quando tem voz)
**da própria campanha**, e não contra a base da 0.15.1: a 0.16 sobe a sala
pessoal ao abrir, e o `sem-mod` dela já carrega esse custo (LEIA, «Contra o que
se lê o custo da API 6»). A `carga-cartoes` é a exceção: ela se lê contra a
`lista-251`, com os mesmos 251 e sem MOD (`REFERENCIA`, abaixo), e a coluna
«contra» diz, em cada linha, contra quem. O jitter é lido só nos cenários com
voz: sem falante, o registro fica no zero inicial.

Confere antes de ler: o mesmo commit em todos, nenhum interrompido, nenhuma
compilação durante a janela. Um relato que não passa é dito, e a tabela não sai.
"""

import json
import sys

# Contra quem cada cenário se lê: o `sem-mod` (ou o `sem-mod+voz`), e a
# `carga-cartoes` contra a `lista-251`, com os mesmos calados e sem MOD
# (decisão da orquestradora, 11/10). Contra o `sem-mod` de uma pessoa, a lista
# de 251 entraria na conta do MOD; a `lista-251` contra o `sem-mod` diz, à
# parte, o custo da lista.
REFERENCIA = {"carga-cartoes": "lista-251"}


def main():
    relatos = json.load(open(sys.argv[1]))
    commits = {r["commit"] for r in relatos}
    sujos = [r["cenario"] for r in relatos if r["interrompido"] or r["rustc_durante"]]
    if len(commits) != 1 or sujos:
        sys.exit(f"relatos que não servem: commits {sorted(commits)}, sujos {sujos}")
    por_nome = {r["cenario"]: r for r in relatos}
    print(f"commit {commits.pop()}, {len(relatos)} cenários\n")
    print("| cenário | contra | CPU % | Δ CPU | MiB | Δ MiB | commits/s | p95 ms | acima de um quadro | jitter mediana ms | Δ jitter |")
    print("|---|---|---|---|---|---|---|---|---|---|---|")
    for r in relatos:
        base = r["cenario"].partition("+")[0]
        nome_da_ref = REFERENCIA.get(base, "sem-mod") + ("+voz" if r["voz"] else "")
        if nome_da_ref not in por_nome:
            sys.exit(f"{r['cenario']} se lê contra {nome_da_ref}, e ele não está entre os relatos")
        ref = por_nome[nome_da_ref]
        q = r["quadros"]
        jitter = r["jitter_do_registro_ms"]["mediana"] if r["voz"] else None
        ref_jitter = ref["jitter_do_registro_ms"]["mediana"] if r["voz"] else None
        print("| {} | {} | {:.1f} | {:+.1f} | {:.0f} | {:+.0f} | {:.1f} | {:.2f} | {} | {} | {} |".format(
            r["cenario"], nome_da_ref, r["cpu_pct"], r["cpu_pct"] - ref["cpu_pct"],
            r["memoria_mib"]["total"], r["memoria_mib"]["total"] - ref["memoria_mib"]["total"],
            q["commits_por_s"], q["commit_ms"]["p95"], q["acima_de_um_quadro"],
            "—" if jitter is None else f"{jitter:.2f}",
            "—" if jitter is None else f"{jitter - ref_jitter:+.2f}",
        ))


if __name__ == "__main__":
    main()
