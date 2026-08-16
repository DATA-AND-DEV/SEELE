# O que saiu da ferramenta de design

Material de origem, guardado como referência. **Nada aqui é servido, compilado
ou lido em tempo de execução** — o que o produto usa é derivado disto e mora
noutro lugar.

Veio de dois arquivos que ficavam soltos na raiz do repositório, `SEELE
Design.zip` e `Aguardando respostas.zip`, apagados depois de conferido, arquivo
por arquivo, o que deles já existia em `design/`. Só o que **não** existia está
aqui; o resto — os dois `Entry Plug*.dc.html`, o `support.js`, o `_ds/` e o
`uploads/` — era byte a byte idêntico ao que já estava versionado, e duplicar
seria criar duas cópias para divergirem depois. Ficou de fora também um
`.thumbnail`, miniatura JPEG que a ferramenta gerava do próprio comp.

---

## `magi-tokens.css` e `magi-tokens.json` — leia antes de comparar

**Estes tokens são da v1 e não são a paleta do produto.** A paleta canônica é o
ADR 0014, e ela vive em `apps/seele-app/ui/tokens.css` com espelho em
`design/seele-tokens.css`.

Isto está escrito porque a confusão já custou caro: alguém comparou a paleta do
app contra este arquivo, concluiu que o app estava errado, e chegou a reverter
uma decisão de marca por causa disso. O comp v2 usa `#F2521F`, `#FF1A1A` e
`#6BFFB6` — exatamente o que o app já tinha. Quem quiser conferir a paleta do
produto compara contra `tokens.css`, nunca contra este arquivo.

O que estes dois servem é para ler a v1 quando alguém perguntar por que um valor
mudou. `.superpowers/sdd/comp-inventario.md` §11 já mediu a diferença entre os
dois e listou item a item.

## `marca/` — os arquivos do desenhista

O `README.md` daqui é dele, não meu, e é a especificação da marca reduzida por
faixa de tamanho. Ele concorda com `docs/marca.md`, que é o normativo: as mesmas
quatro faixas, as mesmas seis placas, o mesmo vermelho reservado a falha.

Os nomes `seele-*` são os da exportação. O produto renomeou ao derivar:
`design/marca/muda.svg` e `design/marca/reduzida.svg` são as formas que
`docs/marca.md` governa e que `apps/seele-app/tests/marca.rs` confere. **Os
arquivos daqui não são intercambiáveis com aqueles** — são a origem, e a marca
seguiu evoluindo depois desta exportação.

Os quatro `icone-app-*.svg` da exportação **não** estão aqui: são byte a byte
iguais aos de `design/marca/`, que é onde `gerar-icones.py` os lê.

### Os dois de bandeja não têm implementação

`icone-bandeja.svg` (plug inserido, barra cheia) e `icone-bandeja-ejetado.svg`
(plug ejetado, barra vazia) são monocromáticos e herdam a cor do tema por
`currentColor`. `docs/marca.md` cita a bandeja do sistema como uso da forma
muda, mas o produto ainda não põe ícone em bandeja nenhuma. Estão guardados
porque, no dia em que puser, o desenho já existe e não precisa ser inventado de
novo.
