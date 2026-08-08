# 06 — Clientes gráficos

## Posição no projeto

Os clientes gráficos existem para alcançar quem não vive no terminal e para dar mobilidade. Eles **imitam a TUI**, não o contrário. Nenhuma funcionalidade nasce aqui: se algo é útil, é implementado em `magi-core` e aparece nas duas interfaces.

## Desktop — Tauri

**Decisão:** Tauri, não Electron.

- Usa a webview do sistema; binário na casa de dezenas de MB, não centenas.
- O núcleo Rust roda no processo nativo, não em JS. Áudio, QUIC e estado ficam onde já estão.
- O frontend é apenas apresentação, comunicando por comandos e eventos Tauri, espelhando o contrato de `magi-core` descrito em `01`.

Frontend: **[EM ABERTO]** — Svelte, Solid ou HTML/CSS puro com um pouco de TS. Dado que a interface é densa mas com pouca interação complexa, algo leve serve melhor que React.

Requisito não negociável: **nenhuma lógica de protocolo em JavaScript**. Se o frontend precisa saber o que é um `ssrc`, algo está errado.

## Mobile

**Escopo em v1: somente consumo.** Ouvir, falar, ler e responder texto. Sem administração, sem gerenciamento de canais, sem configuração avançada.

Justificativa: áudio em background no iOS e Android é trabalhoso (interrupções por chamada, foco de áudio, restrição de bateria, notificação persistente obrigatória). Fazer isso bem já é o milestone inteiro; tentar paridade completa junto atrasa tudo.

**[EM ABERTO — decisão de plataforma]**

| Opção | A favor | Contra |
|---|---|---|
| Flutter + FFI | Uma base para as duas plataformas, boa performance de UI | Ponte FFI com áudio em background é área de pouca trilha batida |
| Nativo (Swift + Kotlin) | Controle total sobre áudio em background, que é o problema difícil | Dobra o trabalho de interface |
| Tauri Mobile | Reaproveita o frontend do desktop | Imaturo; risco alto para áudio em tempo real |

Recomendação: decidir **depois de M4**, com um protótipo descartável de áudio em background em cada candidata. Não decidir por afinidade prévia.

## Camada FFI (`magi-ffi`)

Superfície mínima e estável, gerada com `uniffi` sempre que possível:

```
conectar(host, credencial) -> Sessao
inserir_plug(cage_id)
ejetar_plug()
enviar_mensagem(linha_id, corpo)
definir_at_field(bool)
assinar_eventos(callback)
estado_atual() -> Snapshot
```

Regras:
- Objetos opacos com handle; nada de expor estruturas internas.
- Eventos entregues por callback em thread própria; a casca marshala para sua thread de UI.
- Erros como enum, nunca string.
- Zero dependência da FFI em tipos de UI.

## Paridade de interface

O design gráfico (ver `PROMPT-CLAUDE-DESIGN.md`) tem liberdade tipográfica e de movimento que o terminal não tem. Mas a **composição** — quais painéis existem, o que fica em cada um, onde a telemetria vive — deve ser reconhecivelmente a mesma. Alguém que usa a TUI deve abrir o app e saber onde tudo está sem procurar.

Toda tela gráfica precisa ter uma resposta para: "como isso ficaria em 80×24 monocromático?" Se não tiver, está fora do produto.

## Critérios de aceite

- Desktop: binário abaixo de 30 MB, RSS abaixo de 150 MB, inicialização abaixo de 2 s.
- Mobile: áudio sobrevive a bloqueio de tela, chamada telefônica recebida e troca de rede.
- Ambos: mesma sessão pode ser retomada em outro cliente sem perda de histórico.
