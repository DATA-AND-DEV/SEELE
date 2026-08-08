# 0009 — Orçamento de latência boca-a-ouvido

Status: aceito
Contexto: `specs/00-visao-geral.md` fixa < 60 ms em LAN. `specs/03-audio.md` define quadro Opus de 20 ms e alvo inicial de jitter buffer de 40 ms. Só esses dois somam 60 ms, **antes** de captura, encode, rede, decode e reprodução. `specs/09-roadmap.md` transforma os 60 ms em critério verificável de M1. Como estava escrito, M1 não podia passar (contradição C1 do plano).
Decisão: o número solto é substituído por um orçamento por estágio, e o aceite de M1 passa a ter dois níveis:

- **< 70 ms** com o jitter buffer no piso de 20 ms, em LAN limpa;
- **< 90 ms** com o alvo padrão de 40 ms.

Os números abaixo já são medidos, não estimados. A primeira redação deste ADR
propunha 60 ms no nível piso; `M1.1` mostrou que o piso real é ≈ 67 ms.

| Estágio | Piso | Padrão | Origem |
|---|---|---|---|
| Dispositivo (captura + reprodução + conversores) | 19,6 ms | 19,6 ms | **medido em `M1.1`** |
| Acúmulo de quadro | 20 ms | 20 ms | `specs/03-audio.md` |
| Lookahead do encoder | 6,5 ms | 6,5 ms | **medido em `M0.4`** |
| Encode | 0,04 ms | 0,04 ms | **medido em `M0.10`** |
| Rede (LAN) | < 1 ms | < 1 ms | — |
| Jitter buffer | 20 ms | 40 ms | `specs/03-audio.md` |
| Decode | 0,01 ms | 0,01 ms | **medido em `M0.10`** |
| **Total** | **≈ 67 ms** | **≈ 87 ms** | |

**Revisão após `M1.1`.** A tabela acima é medida, não estimada. Três consequências:

1. **O nível "piso" precisa ser 70 ms, não 60 ms.** O piso medido é ≈ 67 ms, e
   prometer 60 ms seria repetir o erro que este ADR existe para corrigir.
2. **A alternativa (a) está morta no macOS.** Forçar buffer menor que o do
   hardware faz o `cpal`/CoreAudio inserir camada de adaptação e a latência
   **sobe**: 512 frames dão 20,6 ms, 128 dão 36,6 ms, 64 dão 43,1 ms. Não há
   alavanca de buffer a puxar. O efeito se reproduz em `cpal` 0.16 e 0.18, então
   não é artefato de versão. Falta confirmar WASAPI e PipeWire.
3. **A versão do `cpal` vale milissegundos.** 0.16 media 24,4 ms; 0.18 mede
   20,6 ms no mesmo hardware. Atualizar `cpal` é uma alavanca de latência real e
   deve ser tratada como tal, não como manutenção de rotina.

Ressalva aberta: medido com alto-falante e microfone internos. O ADR 0007 exige
fone, o que elimina o ~1 ms de ar e o processamento de proteção do alto-falante
da Apple. O número pode cair — remedir antes de fechar.

Alternativas: (a) manter 60 ms como portão duro, o que exigiria configuração de buffer de baixa latência por backend — `kAudioDevicePropertyBufferFrameSize` no CoreAudio, modo exclusivo ou `IAudioClient3` no WASAPI, `PIPEWIRE_LATENCY` no PipeWire — somando 3 a 5 pontos a M1 sem garantia de fechar; (b) relaxar o alvo de `00` para 80 ms e assumir isso publicamente.
Consequências: M1 passa a ter critério atingível e ancorado em medição. O piso de ≈ 67 ms deixa claro que os 60 ms de `specs/00-visao-geral.md` não são atingíveis com este desenho — o quadro de 20 ms e o lookahead de 6,5 ms sozinhos já consomem quase metade do orçamento, antes de qualquer dispositivo.

**O lookahead de 6,5 ms foi surpresa.** Não está em spec nenhuma e sozinho consome mais de um décimo do orçamento de LAN. Foi obtido de brinde ao provar o toolchain em `M0.4`.

**Aplicado nas specs.** `specs/00-visao-geral.md` passou de 60 para 70 ms em LAN, com nota explicando a origem do número antigo; `specs/09-roadmap.md` teve o aceite de M1 reescrito.

A alavanca de **quadro de 10 ms** foi considerada e recusada. Ela levaria o piso a ≈ 57 ms e cumpriria a promessa original, mas ao custo de dobrar a taxa de pacotes (50 → 100/s), quase dobrar o overhead relativo de cabeçalho (12% → 22%) e dobrar a carga de datagramas no servidor — que `specs/00-visao-geral.md` limita a 15% de 1 vCPU com 20 falantes. Trocar 10 ms de latência por o dobro de trabalho no servidor não se paga na escala alvo.
