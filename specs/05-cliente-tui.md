# 05 — Cliente TUI (`plug`)

O produto principal. Tudo o mais imita esta interface.

## Stack

`ratatui` + `crossterm`. Renderização a ~30 fps apenas quando há mudança (redesenho sob demanda, não loop cego). Suporte a truecolor com degradação para 256 e 16 cores.

## Layout principal

```
┌ MAGI ─────────────────────── 同期率 ─── 第3新東京市 ─────── 12:04:33 ┐
│ DOGMA          │ CAGES / LINHAS       │ MENSAGENS                  │
│ ▸ Terceira Tó… │ ▼ CAGE-01 CENTRAL    │ 12:01 ayanami              │
│   Geofront     │   ● ayanami    98%   │   verificando harmônicos   │
│   Matsushiro   │   ● shinji     71%   │                            │
│                │   ○ asuka    A.T.    │ 12:03 shinji               │
│                │ ▼ CAGE-02 TESTE      │   sync caiu aqui           │
│                │ ─ LINHA #geral       │                            │
│                │ ─ LINHA #logs        │ ▸ _                        │
├────────────────┴──────────────────────┴────────────────────────────┤
│ SYNC 94% │ RTT 38ms │ JIT 12ms │ LOSS 0.2% │ OPUS 32k │ A.T. OFF   │
└────────────────────────────────────────────────────────────────────┘
```

Três painéis verticais mais uma barra de telemetria fixa. Larguras assimétricas e ajustáveis. A telemetria é permanente, não escondida em menu — é a diferença de caráter em relação a um cliente de chat comum.

## Modelo de interação

Modal, no espírito do Vim, porque o público é esse:

| Modo | Entrada | Comportamento |
|---|---|---|
| **Normal** | padrão | Navegação por teclas simples |
| **Inserção** | `i` ou Enter no campo | Digitação de mensagem |
| **Comando** | `:` | Comandos explícitos |
| **Busca** | `/` | Busca no histórico |

Atalhos essenciais no modo Normal:

```
h j k l / setas   navegar
Tab               alternar painel
Enter             inserir plug no Cage / abrir Linha
i                 escrever mensagem
Espaço (hold)     push-to-talk
m                 alternar A.T. Field (mudo)
d                 alternar surdo
g / G             topo / fim do histórico
?                 ajuda
:q                ejetar e sair
```

Comandos: `:conectar <host>`, `:cage <nome>`, `:sync` (diagnóstico detalhado), `:audio` (dispositivos), `:tema`, `:sobre`.

**Resolvido em M4, e eram duas causas independentes.** A colisão com digitação: PTT só no modo Normal, onde não há nada com que colidir (decisão D19). E uma que esta spec não previa: **a maioria dos terminais não reporta soltura de tecla**, então "segurar espaço" abre um microfone que nunca fecha. Onde o protocolo de teclado do Kitty existe, é segurar de verdade; onde não existe, a barra vira trava — aperta para abrir, aperta para fechar (ADR 0016). A barra de telemetria diz qual estado está valendo nos dois casos.

Tecla dedicada configurável não resolveria: o problema não é *qual* tecla, é que nenhuma tem soltura nesses terminais.

## Estados visuais que precisam existir

1. **Boot** — sequência de inicialização, três subsistemas reportando, barra de sincronização subindo. Dura o tempo real da conexão; se conectar em 200 ms, não inventar espera artificial. Animação decorativa que atrasa o usuário é falha de design.
2. **PADRÃO: LARANJA** — conectado, não autenticado.
3. **PADRÃO: AZUL** — operação normal.
4. **Falando** — destaque no roster, indicador de nível.
5. **Bateria interna** — desconectado, contagem 04:59 regressiva, interface esmaecida mas legível, tentativas listadas.
6. **Alerta** — banner 警告 para menção direta ou evento crítico.

## Restrições de renderização

- Tudo alinhado a células. Bordas com box-drawing (`│ ─ ┌ ┼ ╮`).
- Barras com blocos (`█ ▓ ▒ ░` e `▁▂▃▄▅▆▇` para nível de áudio).
- Sem imagem, sem fonte customizada. Ênfase disponível: cor, negrito, inverso, sublinhado.
- Kanji ocupa duas células — calcular largura com `unicode-width`, nunca com `.len()`. Isso vai quebrar o layout se esquecido.
- Terminal mínimo suportado: 80×24. Abaixo disso, degradar para painel único com aviso.

## Acessibilidade

- Modo alto contraste e modo sem cor (só forma e texto) — daltonismo é comum no público e a paleta depende muito de vermelho/verde.
- Nenhuma informação transmitida **só** por cor: a Taxa de Sincronização é sempre acompanhada do número; A.T. Field tem marcador textual além da cor.
- Respeitar `NO_COLOR`.
- **[EM ABERTO]** Leitor de tela em TUI é limitado. Investigar viabilidade mínima.

## Critérios de aceite

- RSS abaixo de 60 MB em operação normal.
- Sem tremulação ao redimensionar.
- Funciona por SSH em terminal de 16 cores sem perder informação.
- Do lançamento até pronto para falar em menos de 1,5 s.
