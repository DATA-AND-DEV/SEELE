# spike `casca-do-celular` — o desenho da tela do celular, tocável

**Descartável.** Existe para o desenho de
[`docs/superpowers/specs/2026-10-09-0.16-celular-design.md`](../../docs/superpowers/specs/2026-10-09-0.16-celular-design.md)
poder ser tocado, fotografado e revisado antes de a casca ser escrita na
`apps/seele-app/ui/` (Etapa B da F8). Nada daqui vai para o produto: na casca, o
celular reorganiza os nós que o desktop já desenha (a arquitetura híbrida do spec,
§9), e não estes.

## As duas peças

**O protótipo** (`index.html`, `celular.css`, `dados.js`, `prototipo.js`):
- o desenho inteiro, com dados de exemplo nos nomes de campo do `Snapshot`;
- lê de `apps/seele-app/ui/` as fontes, os tokens congelados, a `base.css`, a
  `acessibilidade.css`, os glifos, o `frases.js` e a marca, sem cópia nenhuma;
- toda regra de `celular.css` começa pela raiz `data-casca="celular"`, que é a regra
  da casca;
- nenhum `innerHTML`: tudo é montado por `createElement`;
- cada estado tem um endereço (`?tela=sessao&aba=pessoas&tema=papel`), e o
  `fotos.py` lista todos.

**A prova sobre a casca** (`prova-sobre-a-casca/`): a arquitetura híbrida contra a
`ui/` de hoje, por uma cópia, como o `tools/carga-da-casca.py`. Ela prova três
coisas:
- o desktop fica igual pixel a pixel com os arquivos do celular carregados;
- a aba PESSOAS sai da `#lista-roster` do desktop;
- o MUDO da faixa manda `set_muted` pelo `#botao-mudo` de sempre.

## Como roda

```sh
python3 -m http.server 8611 --bind 127.0.0.1           # na raiz do repositório
# http://127.0.0.1:8611/spikes/casca-do-celular/?tela=entrar
python3 spikes/casca-do-celular/fotos.py /tmp/fotos-do-celular
python3 spikes/casca-do-celular/prova-sobre-a-casca/prova.py /tmp/prova
python3 spikes/casca-do-celular/prova-sobre-a-casca/prova.py /tmp/prova --sem-viewport   # a reversão: reprova
```

Precisa do Playwright com o Chromium (`python3 -m pip install playwright` e
`python3 -m playwright install chromium`) e do Pillow, para a comparação de pixels.

## O que o `fotos.py` mede

São 95 casos:
- 31 estados × 390 × 844 e 360 × 800;
- 7 estados nos temas PAPEL, PROFUNDO, hostil e claro;
- 4 estados com o texto a 150 %;
- a tela larga.

Em cada caso ele mede:
- rolagem lateral: só um trilho marcado `data-rola-de-lado` rola de lado;
- alvos abaixo de 44 × 44;
- texto ou controle na ilha ou na barra de gestos, com as áreas seguras de um iPhone
  e de um Android injetadas nas variáveis `--cel-seguro-*`, porque o Chromium não as
  tem;
- texto que passa da própria caixa;
- **o contraste de todo texto visível** contra o fundo opaco real atrás dele;
- **o núcleo:** MUDO, SURDO, SAIR, o selo da conexão segura, a aba AJUSTES e a
  linha dos MODs, com contraste e a mesma cor computada com e sem tema;
- erros de script.

Os 95 passavam em 09/10. Cada medida foi vista reprovar um defeito de verdade antes
do conserto que a fez passar, e a lista está no spec, §11.

## O que ele não prova

O desenho foi medido no Chromium. Ficam por conferir no Safari do Simulador e no
aparelho o texto dinâmico, as áreas seguras reais e o teclado (spec, §12).
