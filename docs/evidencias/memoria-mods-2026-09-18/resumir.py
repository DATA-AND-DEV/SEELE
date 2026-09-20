from pathlib import Path
import json,statistics
root=Path(__file__).parent
summary=[]
def cpu(s):
    parts=s.split(':')
    return sum(float(x)*60**i for i,x in enumerate(reversed(parts)))
def psrows(row):
    return {int(p[0]):dict(rss_kib=int(p[1]),cpu_s=cpu(p[2])) for line in row['ps'].splitlines() if (p:=line.split(maxsplit=3))}
for f in sorted(root.glob('*.json')):
    if f.name in ('ambiente.json','resumo.json'): continue
    rows=json.loads(f.read_text())
    vals=[r['summary_footprint_bytes']/2**20 for r in rows if r.get('summary_footprint_bytes') is not None]
    if not vals: continue
    procs={}
    for r in rows:
        for p in r['processes']: procs.setdefault(p['name'],[]).append(p['footprint_bytes']/2**20)
    first,last=psrows(rows[0]),psrows(rows[-1])
    elapsed=rows[-1]['unix_time']-rows[0]['unix_time']
    cpu_pct=sum(last[p]['cpu_s']-first[p]['cpu_s'] for p in first.keys()&last.keys())/elapsed*100 if elapsed else None
    item=dict(phase=f.stem,n=len(vals),footprint_MiB=dict(median=statistics.median(vals),min=min(vals),max=max(vals)),process_medians_MiB={k:statistics.median(v) for k,v in procs.items()},mean_cpu_percent_of_one_core=cpu_pct)
    summary.append(item)
    print(f'{f.stem}: n={len(vals)} median={statistics.median(vals):.2f} MiB range={min(vals):.2f}-{max(vals):.2f}, CPU={cpu_pct or 0:.2f}%')
(root/'resumo.json').write_text(json.dumps(summary,indent=2))
