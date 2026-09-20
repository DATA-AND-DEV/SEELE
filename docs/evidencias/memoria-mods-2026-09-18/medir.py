import subprocess, re, json, sys, time
from pathlib import Path
phase=sys.argv[1]
pids=sys.argv[2:]
out=Path(__file__).parent
rows=[]
for i in range(7):
    start=time.time()
    cmd=['/usr/bin/footprint', *sum((['-p',p] for p in pids),[]),'--noCategories','-f','bytes']
    r=subprocess.run(cmd,capture_output=True,text=True)
    (out/f'{phase}-{i}.txt').write_text(r.stdout+r.stderr)
    samples=[]
    for m in re.finditer(r'^(.+?) \[(\d+)\]:.*?Footprint: (\d+) B',r.stdout,re.M):
        samples.append(dict(name=m[1],pid=int(m[2]),footprint_bytes=int(m[3])))
    ps=subprocess.check_output(['ps','-p',','.join(pids),'-o','pid=,rss=,time=,comm='],text=True)
    total=re.search(r'Summary Footprint: (\d+) B',r.stdout)
    row=dict(phase=phase,index=i,unix_time=start,elapsed=time.time()-start,processes=samples,summary_footprint_bytes=int(total[1]) if total else None,ps=ps,returncode=r.returncode)
    rows.append(row)
    (out/f'{phase}.json').write_text(json.dumps(rows,indent=2))
    print(phase,i,round(row['summary_footprint_bytes']/2**20,2) if total else 'ERROR',flush=True)
    if i<6: time.sleep(max(0,5-(time.time()-start)))
