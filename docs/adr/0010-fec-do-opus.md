# 0010 — FEC in-band do Opus desligado em v1

Status: aceito
Contexto: `specs/03-audio.md` listava "FEC in-band: **Ligado**" como parâmetro fechado, enquanto `specs/02-protocolo.md` perguntava em `[EM ABERTO]` se FEC entrava em v1. A spec se contradizia consigo mesma (contradição C4 do plano). A primeira redação deste ADR ficou `proposto` esperando dados; `M1.7` produziu os dados.
Decisão: **FEC in-band desligado em v1.** Só jitter buffer com PLC, como `specs/02-protocolo.md` sugeria na alternativa. DTX permanece ligado.
Alternativas:

- **Ligado sempre**, como a tabela de `03` dizia. Custo fixo de +20 ms em todo cenário, inclusive LAN limpa, empurrando o piso do ADR 0009 de ≈ 67 ms para ≈ 87 ms.
- **Adaptativo por perda medida.** Atraente e provavelmente o destino final, mas hoje está bloqueado por dois lados: não existe canal de realimentação receptor→emissor até M2, e o `shiguredo_opus` (ADR 0008) não expõe setter em tempo de execução. Fica como dívida registrada de M2.

Consequências: o mecanismo do FEC in-band (LBRR) só funciona se o decoder receber o pacote **seguinte** ao perdido, o que obriga o jitter buffer a segurar um quadro extra.

Medido em `M1.7` (`cargo run --release -p magi-audio --example jitter_profiles`):

| perfil | perda de rede | alvo do buffer |
|---|---|---|
| `lan` | 0,00 % | **20,0 ms** (piso) |
| `acceptance 5%` | 4,48 % | **20,0 ms** (piso) |
| `wifi` | 1,09 % | 51,2 ms |
| `mobile_poor` | 4,34 % | 120,8 ms |

Em LAN o buffer assenta exatamente no piso de 20 ms e a perda é zero. Ligar FEC dobraria a profundidade — um terço do orçamento de LAN — para proteger contra perda inexistente. Nos perfis onde há perda, o buffer já compra profundidade por conta própria, e ali o trade-off provavelmente se inverte.

Reavaliar em M2, com medição de perda em internet regional e canal de realimentação disponível.

Custo de reverter: **médio**. Ligar FEC muda a profundidade mínima do jitter buffer e portanto o orçamento do ADR 0009.
