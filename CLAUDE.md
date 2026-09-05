# Como trabalhar neste repositório

## Antes de diagnosticar, confira o que está publicado

**A regra existe porque eu a quebrei duas vezes no mesmo dia.** Nas duas, expliquei
um defeito com «esse conserto ainda não chegou até você» — e ele tinha chegado:
quem opera já havia publicado a versão nova. A segunda vez veio com «isso já está
ficando irritante», e estava certo: um diagnóstico que começa supondo o que a
outra pessoa tem é um diagnóstico que a acusa de não ter feito o que ela fez.

Antes de dizer qualquer coisa sobre o que está ou não está numa máquina:

```sh
curl -sS "https://api.github.com/repos/DATA-AND-DEV/SEELE-RELEASES/releases" \
  | python3 -c "
import json,sys,re
for r in json.load(sys.stdin)[:6]:
    m = re.search(r'commit \`([0-9a-f]{7,40})\`', r.get('body') or '')
    print(r['tag_name'], '·', m.group(1)[:9] if m else '?', '·', r.get('published_at'))
"
```

Cada release grava no corpo o commit de que saiu. Com ele, `git log <commit>` diz
exatamente o que está lá dentro — e a resposta deixa de ser uma suposição sobre a
máquina de outra pessoa.

**O mesmo vale para o que a versão carrega**: qual protocolo, qual instalador o
manifesto aponta, que sistemas foram publicados. Tudo isso se lê do release, e
nada disso se adivinha.

## O que mais custou caro aqui

- **«O produto sabe e não conta.»** A maior parte dos defeitos deste repositório
  não foi o produto errar: foi ele acertar e não dizer. Uma falha silenciosa
  volta como pergunta de quem usa, dias depois, sem dado nenhum junto.
- **«Existir não é funcionar.»** Um guarda que casa com o próprio comentário, um
  teste dentro de um `#![cfg(windows)]` que nunca roda no Mac, um contador que
  ninguém lê. Provar um guarda contra a regressão de verdade — revertendo o
  conserto e vendo-o falhar — é o que separa os dois.
- **Medir antes de concluir.** Três vezes uma hipótese confiante custou mais que
  a medida teria custado.
