# 0016 — Push-to-talk vira trava onde o terminal não reporta soltura de tecla

Status: aceito

Contexto: `specs/05-cliente-tui.md` pede `Espaço (hold)` para push-to-talk e marca **[EM ABERTO]** a colisão com digitação. A decisão D19 já resolvia a colisão — PTT só no modo Normal, onde não há nada com que colidir. Ao implementar M4.3 apareceu um problema que a spec não previu e que é anterior a esse: **a maioria dos terminais não informa quando uma tecla é solta.**

Eventos de soltura só existem no protocolo de teclado do Kitty (`CSI > 1 u`, `REPORT_EVENT_TYPES`), suportado por kitty, foot, WezTerm, Ghostty e pouco mais. Terminal.app, iTerm2, o Terminal do GNOME em configuração padrão e praticamente tudo a que alguém chega por SSH mandam apenas o pressionamento. Num terminal desses, "segurar espaço" é indistinguível de "apertar espaço" — e um microfone aberto por um evento que nunca recebe o seu par é um microfone que nunca fecha.

Decisão: consultar `supports_keyboard_enhancement()` **uma vez**, no arranque, e escolher o comportamento a partir da resposta:

- **Terminal que reporta soltura:** espaço segurado abre, espaço solto fecha. É o que a spec pede, literalmente.
- **Terminal que não reporta:** espaço vira trava. Aperta para abrir, aperta de novo para fechar.

Nos dois casos a barra de telemetria diz qual estado está valendo, e o roster mostra `●` para quem está transmitindo. A diferença é no tato, não na informação.

Alternativas:

1. **Tecla dedicada configurável**, como a spec sugere. Não resolve nada: o problema não é *qual* tecla, é que nenhuma tecla tem soltura nesses terminais.
2. **VAD como padrão para todo mundo.** Descartado porque `specs/03-audio.md` escolhe PTT como padrão justamente por nunca disparar sozinho, e um cliente que transmite uma sala por engano é a falha mais cara das duas. Continua disponível em `:voz vad`.
3. **Timeout: abrir no pressionamento e fechar sozinho depois de N ms sem repetição.** Foi a alternativa mais tentadora, porque preserva o gesto. Descartada porque a taxa de auto-repetição do teclado é configuração do sistema operacional e varia de ~25 ms a desligada; calibrar um timeout contra isso significa cortar o fim de frase de quem tem repetição lenta, ou deixar o canal aberto para quem a desligou.
4. **Exigir um terminal com o protocolo do Kitty.** Contraria o critério de aceite de M4 — funcionar por SSH em terminal de 16 cores — e o público que a spec descreve.

Consequências:

- Duas experiências de PTT em campo. Isso é real e é o custo. A mitigação é que ambas são estados visíveis e explícitos, não modos escondidos: quem abre o cliente num terminal com trava vê a barra dizendo que está transmitindo, e vê o próprio `●` no roster.
- A consulta é feita **uma vez**. Chamar `supports_keyboard_enhancement()` por uso custa uma pergunta ao terminal e a espera pela resposta a cada uso; num terminal que não responde, custa o timeout inteiro. Esse foi um travamento real de 2,4 s no arranque antes de ser corrigido — medido, não hipotético.
- `specs/05-cliente-tui.md` deveria registrar que o item **[EM ABERTO]** tem duas causas independentes, e que esta ADR resolve a segunda. A primeira continua resolvida por D19.

Custo de reverter: **baixo**. O ramo inteiro está em duas funções de `crates/seele-tui/src/main.rs`, e o modelo em `app.rs` só conhece `SpaceDown` e `SpaceUp` — ele não sabe nem se importa com qual dos dois caminhos os produziu.
