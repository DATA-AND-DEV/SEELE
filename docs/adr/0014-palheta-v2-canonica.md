# 0014 — Palheta v2 como canônica

Status: aceito por default
Contexto: o Claude Design entregou dois protótipos e um arquivo de tokens. `seele-tokens.*` estava em sincronia com o **v1**; o **v2**, artefato mais recente, revisou três cores. Duas palhetas concorrentes no mesmo repositório garantem que alguém construa contra a errada.
Decisão: a palheta do v2 é canônica. `design/seele-tokens.json` e `.css` foram regenerados a partir dela, com índices ANSI 256 recalculados e contraste medido.

| token | v1 | v2 |
|---|---|---|
| `laranja-nerv` | `#FF6B00` | `#F2521F` |
| `vermelho-alerta` | `#E01B24` | `#FF1A1A` |
| `fosforo` | `#3DF57A` | `#6BFFB6` |

Alternativas: manter v1, que era o que os tokens já diziam. Descartado por um motivo que só apareceu ao medir: em v1 o `vermelho-alerta` ficava em **4,14:1, reprovando WCAG AA** — e vermelho é justamente a cor que `specs/07` reserva para erro e queda. O v2 leva para 5,16:1. A revisão não foi só de gosto.

Consequências:

- Mais fácil: uma palheta só; os índices ANSI passam a ser calculados em vez de declarados, e um deles estava errado — o arquivo entregue dizia `ansi256: 208` para `#FF6B00`, quando o vizinho é 202.
- Mais difícil: `laranja-nerv` perdeu contraste, de 7,00:1 (AAA) para 5,71:1 (AA). Continua aprovado, mas não há mais folga.
- Método registrado nos próprios tokens: vizinho mais próximo em CIELAB, **restrito aos índices 16–255**, porque 0–15 são retematizáveis pelo usuário em qualquer terminal.

Quatro achados viraram trabalho de M4 e estão em `docs/tokens-achados.md`: `osso-apagado` reprovando AA para texto pequeno, painel e fundo colapsando no mesmo ansi 232, linhas abaixo do critério de componente não textual, e a ausência de modo sem cor.

Não adotado: a varredura animada do v2 (`seeleVarredura`), por contrariar "movimento é diagnóstico" de `specs/07`. A textura estática de scanline foi mantida.

Custo de reverter: **baixo** agora, **médio** depois que M4 começar a construir telas.

## Revisão em M5 — a palheta ganhou seis placas, e o JSON não as carrega de propósito

Status: aceito · a decisão acima continua valendo, isto acrescenta seis valores a ela

O ícone de aplicativo de M5 precisou de uma coisa que a palheta congelada não tinha. `docs/marca.md` proíbe sombra e gradiente, e a profundidade do plug é feita de **cor plana deslocada**: cada placa atrás da marca é um degrau de laranja, três escalonando sobre o negro e três sobre o laranja. São cores da arte, e sem elas o ícone não se desenha.

Decisão: `design/seele-tokens.css` passa a declarar seis valores novos, num bloco próprio e rotulado como da marca:

| token | hex | onde |
|---|---|---|
| `--seele-placa-negro-1` | `#A83A10` | placa mais próxima, sobre negro |
| `--seele-placa-negro-2` | `#7A2A0B` | |
| `--seele-placa-negro-3` | `#4A1806` | placa mais distante, sobre negro |
| `--seele-placa-laranja-1` | `#FFA070` | placa mais distante, sobre laranja |
| `--seele-placa-laranja-2` | `#C4400F` | |
| `--seele-placa-laranja-3` | `#8E2A08` | placa mais próxima, sobre laranja |

Elas não revogam nada: as catorze cores de interface seguem exatamente como estavam, e **nenhuma superfície do produto se pinta com uma placa**. O bloco existe para que a marca desenhada não seja o único lugar do repositório com hexadecimal solto — `apps/seele-app/tests/tokens.rs` reprova literal de cor na folha de estilo, e sem token declarado a arte da marca não teria como entrar no app.

**E `design/seele-tokens.json` deliberadamente não as carrega.** Isto é o que a próxima pessoa precisa ler antes de "consertar" a diferença. Todo item de `cor` no JSON tem `ansi256` e `ansi16`, porque o consumidor dele é `crates/seele-tui/src/theme.rs`, que traduz o arquivo à mão em constantes de terminal — cada cor ali existe para poder ser desenhada num terminal degradando de truecolor para 256 e para 16. As placas nunca serão desenhadas num terminal: `docs/marca.md` é explícito de que dentro da TUI a marca é a forma latina, e o plug com profundidade não aparece. Inventar seis índices ANSI para cores que a TUI não pode pintar seria pior que a ausência delas — seriam seis valores calculados, versionados e conferidos por teste sem nenhum consumidor, e o primeiro a mexer no tema teria seis cores plausíveis à disposição para usar por engano.

O par não está fora de sincronia; ele é assimétrico por decisão. **O CSS é a folha de tudo que se pinta, o JSON é a folha do que o terminal precisa saber pintar** — e as placas só existem do primeiro lado.

Alternativa descartada: pôr as seis no JSON com `ansi256: null`. Custaria um campo opcional num arquivo em que hoje todo campo é obrigatório, e um `null` num arquivo de tokens é um convite a alguém preenchê-lo.

Consequências:

- Quem regenerar o par a partir do JSON precisa **preservar o bloco de placas do CSS**. Ele não sai do JSON e não voltaria sozinho.
- `docs/marca.md` lista as seis na tabela de cores da marca, e o teste que confere que a marca não usa cor fora da lista é o que mantém as duas listas iguais.
- Se algum dia a TUI precisar mesmo de uma placa, este ADR é o lugar de registrar a virada — e aí as seis ganham índice, calculado pelo mesmo método de vizinho em CIELAB restrito a 16–255.

Custo de reverter: **baixo**. Nenhuma superfície depende delas; tirar as seis linhas do CSS custa o ícone de aplicativo, não a interface.

## Qual arquivo é o canônico, e o que fazer ao achar um `magi-tokens`

Status: aceito · não muda nenhuma cor, diz onde elas moram

O arquivo de tokens do produto é `apps/seele-app/ui/tokens.css`, cópia byte a
byte de `design/seele-tokens.css` e conferida como tal por
`apps/seele-app/tests/tokens.rs`. É esse par que a decisão acima congelou, e é
contra ele que qualquer tela se mede.

Um export de design pode trazer `magi-tokens.css` ou `magi-tokens.json`. **Eles
são históricos.** Carregam a palheta v1 — `#FF6B00`, `#E01B24`, `#3DF57A` — que
é exatamente o que este ADR substituiu por `#F2521F`, `#FF1A1A` e `#6BFFB6`. Ler
um deles como autoridade não é hipótese: já aconteceu, e a conclusão foi que o
app estava com as cores erradas quando o app estava com as cores certas. O
sintoma é convincente ao contrário — três cores, todas próximas, todas plausíveis,
e o arquivo com cara de fonte de verdade.

A regra é curta: onde um `magi-tokens` discordar do `tokens.css`, quem está
desatualizado é o `magi-tokens`. Ele não é para ser sincronizado nem consertado;
é para ser reconhecido como o que veio antes.
