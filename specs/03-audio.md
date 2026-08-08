# 03 — Áudio

Esta é a parte difícil do projeto. Tratar com prioridade e prototipar cedo.

## Pipeline de captura

```
cpal (callback tempo real)
  └─▶ ring buffer lock-free
        └─▶ thread de processamento
              ├─ conversão para 48 kHz mono f32
              ├─ ganho automático  [EM ABERTO]
              ├─ supressão de ruído  [EM ABERTO]
              ├─ VAD ou push-to-talk
              └─ encoder Opus (quadros de 20 ms)
                    └─▶ datagram QUIC
```

## Pipeline de reprodução

```
datagrams (N fontes)
  └─▶ um jitter buffer por ssrc
        └─▶ decoder Opus por ssrc
              └─▶ mixer (soma + ganho por usuário + clipping suave)
                    └─▶ ring buffer
                          └─▶ cpal (callback tempo real)
```

## Regras invioláveis do thread de tempo real

O callback do `cpal` roda em thread de prioridade alta com deadline de poucos milissegundos. Dentro dele:

- **Sem alocação de heap.** Nada de `Vec::push`, `String`, `format!`, `Box`.
- **Sem `Mutex`** que possa bloquear. Só estruturas lock-free.
- **Sem I/O, sem log, sem `println!`.**
- **Sem `.await`.** O callback não é async.

Violar isso produz estalos audíveis que são difíceis de diagnosticar depois. Sugestão: um teste de CI que falhe se o crate de áudio no caminho crítico usar alocação (ferramentas como `assert_no_alloc`).

## Codec

**Opus**, via `audiopus` ou `opus` (bindings para libopus).

| Parâmetro | Valor |
|---|---|
| Taxa de amostragem | 48 000 Hz |
| Canais | 1 (mono) na captura |
| Tamanho do quadro | 20 ms (960 amostras) |
| Bitrate | 16–48 kbps, adaptativo |
| Modo | VOIP |
| FEC in-band | **Desligado em v1** — ver ADR 0010 |
| DTX | Ligado (economiza banda em silêncio) |

Bitrate adaptativo reage à taxa de perda: cai para 16 kbps sob perda > 5%, sobe
de volta gradualmente. A faixa declarada acima inclui os 16 kbps; a redação
anterior dizia 24–48 e depois mandava cair para 16, o que se contradizia.

**FEC in-band está desligado em v1.** Ele só ajuda se o decoder receber o pacote
*seguinte* ao perdido, o que obriga o jitter buffer a segurar um quadro extra:
+20 ms. `M1.7` mediu que em LAN e no perfil de aceite o buffer assenta exatamente
no piso de 20 ms, então ligar FEC dobraria a profundidade para proteger contra
perda que, em LAN, é zero. Reavaliar em M2, quando houver canal de realimentação
e medição de perda em internet regional. Ver `docs/adr/0010-fec-do-opus.md`.

## Jitter buffer

O componente que separa "funciona" de "funciona bem". Um buffer **adaptativo** por fonte:

- Alvo inicial: 40 ms (2 quadros).
- Mede o jitter de chegada continuamente; ajusta o alvo entre 20 ms e 200 ms.
- Cresce rápido sob instabilidade, encolhe devagar sob estabilidade — assimetria intencional, porque cortar áudio é pior que atrasar.
- Quadro faltante → PLC do Opus (`decode` com `null`) para um quadro; a partir do segundo, silêncio com fade.
- Quadro atrasado além do alvo → descartado, contabilizado como perda.
- Expõe métricas: profundidade atual, perdas, quadros ocultados, descartes tardios. Isso alimenta a Taxa de Sincronização.

Escrever o jitter buffer como módulo **puro e determinístico**, testável sem áudio real: entra sequência de quadros com timestamps, sai sequência de decisões. Isso permite testes de propriedade contra padrões de rede sintéticos.

## VAD e push-to-talk

- **Push-to-talk** é o padrão. Mais previsível, sem falso positivo.
- **Ativação por voz** via `webrtc-vad`, com histerese: limiar de abertura mais alto que o de fechamento, e um *hangover* de ~300 ms para não cortar fim de frase.
- Ambos alimentam o mesmo sinal `falando: bool`, que é anunciado ao servidor para a interface poder destacar quem fala.

Captura de tecla global (PTT com a janela sem foco) é um problema específico por plataforma. **[EM ABERTO]** — no terminal, PTT global exige permissão de acessibilidade no macOS e acesso a `evdev`/portal no Linux/Wayland. Definir se v1 suporta ou exige foco na janela.

## Cancelamento de eco

**[EM ABERTO — decisão importante]** Não existe implementação madura de AEC em Rust puro hoje. Opções:

1. **Exigir fone de ouvido** e documentar isso. Solução honesta, zero código, aceitável para o público-alvo. Recomendada para v1.
2. **Bindings para `webrtc-audio-processing`** (C++). Traz AEC, AGC e supressão de ruído de qualidade, ao custo de build multiplataforma mais complicado.
3. **`speexdsp`** — AEC mais simples e mais leve, qualidade inferior.

Recomendação: v1 sai com opção 1 documentada e a arquitetura preparada para plugar a opção 2 como feature opcional de compilação.

## Dispositivos

- Enumerar entrada e saída via `cpal`; permitir escolha explícita, com "padrão do sistema" como opção.
- Tratar **desconexão de dispositivo em tempo de execução** (fone tirado no meio da chamada). Isso vai acontecer e não pode derrubar a sessão: pausar, reenumerar, retomar no novo padrão, avisar na interface.
- Testar em WASAPI (Windows), CoreAudio (macOS) e ALSA + PipeWire (Linux). PipeWire é o caso mais comum e mais chato.

## Métricas expostas ao restante do sistema

`nivel_entrada`, `nivel_saida`, `falando`, `profundidade_jitter_ms`, `perda_pct`, `quadros_ocultados`, `bitrate_atual`, `rtt_ms`. Tudo isso aparece na barra de telemetria da interface e no cálculo de sincronização.
