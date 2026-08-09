# 0019 — Frontend do desktop em HTML, CSS e TypeScript, sem framework e sem npm

Status: aceito

Contexto: `specs/06-clientes-gui.md` deixa **[EM ABERTO]** — "Svelte, Solid ou HTML/CSS puro com um pouco de TS" — e já dá a direção: "Dado que a interface é densa mas com pouca interação complexa, algo leve serve melhor que React."

Três coisas que só ficaram concretas em M5:

1. **Os tokens já são CSS.** `design/seele-tokens.css` foi congelado em M0.12 com as custom properties, a grade de célula 8×16 e as faixas da Taxa de Sincronização. Qualquer framework consumiria isso via as mesmas variáveis CSS. Nenhum deles agrega nada nessa parte.

2. **O estado não vive no frontend.** A `seele-ffi` entrega um `Snapshot` inteiro e avisa quando ele mudou. Não há estado derivado, não há cache, não há sincronização — a tela é uma função de um valor que chega pronto. É exatamente o caso em que reatividade fina rende menos, porque não há o que reagir finamente.

3. **Um `node_modules` seria a única parte do produto sem auditoria.** O projeto roda `cargo deny` sobre avisos, licenças, bans e fontes em cada build. Trazer um bundler significa trazer uma árvore de dependências que essa guarda não olha, num produto cujo argumento é ser auto-hospedado por quem não confia em terceiros. Isso não é uma objeção estética.

Decisão: HTML, CSS e JavaScript escritos à mão, servidos como arquivos estáticos pelo `frontendDist` do Tauri. Sem framework, sem bundler, sem `package.json`, sem passo de build no frontend.

O padrão de renderização é o mesmo da TUI: **projetar o snapshot inteiro**. Um `render(snapshot)` que reconstrói os painéis, com uma comparação rasa para não redesenhar o que não mudou. É o que `seele-tui::view` faz, e é a razão de os dois lados serem reconhecivelmente a mesma interface.

Alternativas:

1. **Solid.** A escolha mais forte das três: reatividade fina, ~7 kB, sem VDOM. Descartada porque exige Vite e portanto npm, e porque o ganho — atualizar um nó em vez de um painel — resolve um problema que um roster de vinte linhas não tem.
2. **Svelte.** Mesmo custo de toolchain, mais mágica de compilação, e a mesma ausência de problema a resolver.
3. **TypeScript com `tsc` apenas.** Tipos sem bundler é possível, mas `tsc` ainda é npm, e o benefício some quando a única fronteira tipada que interessa — a que vem da `seele-ffi` — é validada em Rust antes de virar JSON.

Consequências:

- Zero dependências de JavaScript. A árvore de suprimentos do produto inteiro é a que o `cargo deny` já cobre.
- Sem passo de build no frontend: editar um arquivo e recarregar. O `cargo tauri dev` fica genuinamente rápido.
- **O custo é real:** manipulação de DOM à mão. Um roster que cresce para centenas de linhas, ou uma lista de mensagens virtualizada, ficariam desconfortáveis. O sinal de que esta decisão expirou é precisar de virtualização — e não é "o arquivo ficou grande".
- Sem verificação de tipos no frontend. A mitigação é que a fronteira que importa é gerada de tipos Rust: se o `Snapshot` mudar de forma, o Rust não compila, e o campo simplesmente não chega ao JavaScript.
- `specs/06-clientes-gui.md` deveria registrar que o item **[EM ABERTO]** está fechado e por qual critério — que acabou não sendo preferência de framework, e sim não ter duas árvores de dependência com uma só auditada.

Custo de reverter: **baixo enquanto for uma tela**. Trocar por Solid é reescrever `render()` em componentes, com os tokens, o HTML e a fronteira da FFI intactos. Sobe junto com o número de telas.
