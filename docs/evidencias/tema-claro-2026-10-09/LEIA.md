# O texto da sessão sob um tema claro (09/10/2026)

Medido pela F3, a pedido da orquestradora, depois do achado da F8: com um tema
claro que a validação aceita (`oTextoAlcancaOFundo()` devolve `true`), o texto
do desktop não acompanhava o tema.

## Como medir

```sh
PLAYWRIGHT=<caminho>/node_modules/playwright node medir.cjs <raiz do checkout>
TEMA='{"fundo":"#f2ece0","painel":"#e6dfd0","texto":"#12100e","apagado":"#5a5148","acento":"#8a4a00","borda":"#b8ad9c"}' \
  PLAYWRIGHT=... node medir.cjs <raiz>
PLAYWRIGHT=... node capturar.cjs <raiz> <prefixo>   # sessão sem tema, 1280 e 900 px
```

`medir.cjs` sobe a interface real da raiz (o `telas.cjs` dela), aplica o tema
pelo `aplicarOTemaDoMod` da API 5, e mede o contraste de **todo** texto visível
dentro de `#tela-sessao` contra o primeiro fundo opaco acima dele. As versões
antigas foram medidas por `git archive <commit> apps/seele-app/ui
apps/seele-app/bancada`.

## Os números

| versão | tema | corpo da mensagem | textos abaixo de 3:1 |
|---|---|---|---|
| v0.15.1 publicada (`54f542f`) | texto e fundo (o da F8) | 1,09:1 | 23 de 82 |
| 0.15.2 (`148c9e60`) | texto e fundo | 1,09:1 | 23 de 82 |
| 0.16 (`0.16/api-6`) | texto e fundo | 1,09:1 | 23 de 84 |
| v0.15.1 publicada | claro completo, seis cores | 1,09:1 | 17 de 82 (SAIR DA SALA e COMPARTILHAR a 1,04:1, o roster a 1,04:1) |
| 0.16, com o conserto | claro completo | **16,14:1** | 11 de 84, todos da tarefa 17e |

**A causa:** o `body` resolve `color: var(--seele-osso)` com o valor da raiz, o
tema redefine o token em `#tela-sessao`, e quem não declara cor herda a cor já
resolvida. **O conserto:** `#tela-sessao { color: var(--seele-osso) }`, em
`tela-sessao.css`.

**Sem tema, nada muda:** as capturas de `capturar.cjs` antes e depois do
conserto são iguais byte a byte, em 1280 e em 900 px (e duas capturas seguidas
sem mudança também, o que mostra que a comparação mede o desenho).

**O que sobra depois do conserto é da 17e**, e não de herança: as cores de
estado (o fósforo da sincronia a 1,05:1 e o azul do nome do operador a 2,58:1
sobre o painel claro) e os tons derivados (o acento do tema sobre o
`--seele-laranja-fraco` do produto, a 2,42:1).

**Quem era atingido na 0.15.1:** os três temas prontos do ESTILO 3.1.0 são
escuros (o PAPEL é fundo `#12100e` e texto `#f2ece0`); só quem monta um tema
claro próprio no editor dele.
