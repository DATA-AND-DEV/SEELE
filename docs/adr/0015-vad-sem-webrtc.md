# 0015 — VAD por energia com histerese, sem `webrtc-vad`

Status: aceito por default
Contexto: `specs/03-audio.md` especifica ativação por voz "via `webrtc-vad`, com histerese: limiar de abertura mais alto que o de fechamento, e um *hangover* de ~300 ms para não cortar fim de frase". A spec nomeia uma biblioteca **e** descreve um comportamento. Ao implementar M1.13 ficou claro que os dois podem ser separados.
Decisão: implementar o comportamento — histerese mais hangover — em Rust puro, sobre energia RMS por quadro. Sem dependência de `webrtc-vad`.
Alternativas: usar `webrtc-vad` como a spec nomeia. Descartado por três motivos que se acumulam:

1. É mais um binding C, com exatamente o perfil de manutenção que fez M0.4 abandonar o `audiopus` — e o `cargo deny` que descobriu aquilo pega este do mesmo jeito.
2. O ADR 0007 já decidiu não trazer DSP em C/C++ para v1. Abrir exceção para VAD reabre a discussão de AEC junto.
3. O que a spec realmente pede é comportamento: abertura acima de um limiar, fechamento abaixo de outro mais baixo, cauda de 300 ms. Isso é detecção por energia com histerese, e é o que `webrtc-vad` faz internamente com mais sofisticação estatística.

Consequências:

- Mais fácil: zero dependências novas, módulo puro, totalmente testável. Há teste para as duas falhas que tornam VAD inutilizável na prática — o portão tremulando num limiar único (200 quadros oscilando ao redor do limiar produzem **uma** abertura) e ruído de sala segurando o canal aberto (500 quadros de ruído de fundo produzem **zero** aberturas).
- Mais difícil: detecção por energia é pior que a do WebRTC em ambiente ruidoso com fala baixa. Se isso doer em uso real, o seam existe — `GateMode::VoiceActivated` é uma variante de enum e trocar a implementação por trás dela não muda nada acima.
- `specs/03-audio.md` deveria ser corrigida: ela nomeia uma biblioteca onde deveria descrever um requisito.

Limiares atuais: abertura em RMS 0,02 (≈ −34 dBFS), fechamento em 0,01 (≈ −40 dBFS), hangover 300 ms. Escolhidos para ficar acima de ruído de sala e ventoinha, abaixo de voz falada baixa. **Não foram validados com microfone e sala reais** — isso entra no checklist de plataforma de M1.15.

Custo de reverter: **baixo**. A fronteira é `GateMode`, e nada fora de `seele-audio::gate` sabe como a decisão é tomada.
