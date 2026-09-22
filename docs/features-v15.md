# SEELE — novas features para a v0.15

Criado em 22/09/2026. Status: **planejado, ainda não implementado por este documento**.

Este arquivo reúne novas funcionalidades solicitadas para a v15 e complementa a [review de código](/Users/dev-alexandre/SEELE/docs/review-v15-2026-09-21.md). O primeiro pacote é melhorar a continuidade e a clareza da voz: falar baixo sem perder palavras e reduzir o ruído enviado pelo microfone.

## F01 · Ativação por voz mais sensível, com alvo de −60 dBFS

**Pedido:** abaixar mais o limiar do modo de voz, pois a conversa ainda corta bastante. Valor sugerido: −60 dBFS.

No código local inspecionado, o gate abre em aproximadamente **−42 dBFS**, fecha abaixo de **−48 dBFS** e mantém a transmissão por **300 ms** após o nível cair. Esses valores estão em [gate.rs](/Users/dev-alexandre/SEELE/crates/seele-audio/src/gate.rs:24). Isso descreve a árvore local, não uma verificação da versão instalada nas máquinas dos participantes.

Aqui, “60 dB” é interpretado como **−60 dBFS**, nível digital relativo à escala máxima. Tornar o limiar mais negativo aumenta a sensibilidade de ativação; não significa diminuir o volume da voz transmitida.

### Comportamento desejado

- Adotar **−60 dBFS como alvo inicial de abertura a validar junto do filtro de microfone F02**, com ajuste manual de sensibilidade nas configurações de áudio.
- Preservar início e fim das palavras, fala baixa e pausas curtas. Avaliar uma pequena retenção dos quadros anteriores à abertura para não perder a primeira sílaba.
- Manter limiares distintos para abrir e fechar, além do tempo de sustentação, ajustados com gravações reais. O valor de −60 dBFS sozinho não resolve todos os cortes.
- Mostrar o nível do microfone e quando a transmissão abre; permitir testar e ouvir o resultado antes de entrar numa conversa. Salvar a preferência localmente.
- Respeitar mute e push-to-talk: maior sensibilidade nunca deve sobrepor esses controles.

### Critérios de aceite

1. Comparar a configuração atual com o alvo de −60 dBFS usando as mesmas gravações de fala baixa, normal e distante, incluindo começo de frase e consoantes finais.
2. Fala baixa acima do limiar configurado abre a transmissão; pausas curtas não fragmentam as frases e mute continua impedindo o envio.
3. Avaliar também teclado, ventilador e silêncio ambiente: registrar ativações indevidas e tempo aberto sem fala, com filtro ligado e desligado.
4. O ajuste persiste após reiniciar; trocar microfone permite conferir novamente o nível e o resultado.

**Ponto de validação:** −60 dBFS é a hipótese inicial solicitada, não um valor ideal já demonstrado. Sem tratamento de ruído, um ambiente acima desse nível pode manter o gate aberto. A mudança deve ser avaliada em conjunto com F02, preservando o ajuste manual caso um único padrão não sirva para todos os microfones.

## F02 · Filtro de microfone para priorizar a voz

**Pedido:** adicionar um filtro de voz ao áudio do microfone, “a la Discord”. A referência é a experiência de conversa limpa; o escopo inicial aqui é **supressão de ruído com preservação da fala natural**.

O caminho local inspecionado já tem [ganho automático](/Users/dev-alexandre/SEELE/crates/seele-audio/src/ganho.rs:1), aplicado após a decisão de ativação em [voice.rs](/Users/dev-alexandre/SEELE/crates/seele-core/src/voice.rs:1777). Esse ganho não remove ruído e não recupera fala descartada pelo gate.

### Comportamento desejado

- Reduzir ruído de fundo do microfone, principalmente ventilador e digitação, preservando fala baixa, timbre e inteligibilidade.
- Oferecer um controle simples **“Redução de ruído”**, com opção de desligar, e teste local para comparar o áudio original e o tratado.
- Processar o microfone localmente antes de enviar; funcionar tanto em ativação por voz quanto em push-to-talk.
- Integrar a supressão à detecção de fala, para que a maior sensibilidade de F01 não apenas transmita mais ruído. Avaliar a ordem proposta: captura → supressão → decisão de ativação → ganho existente → codificação.
- Preservar uma saída sem filtro se o processamento falhar ou não estiver disponível, indicando o estado ao usuário e mantendo mute/PTT válidos.

### Critérios de aceite

1. Comparação com e sem filtro, nas mesmas gravações: ruído diminui sem remover palavras ou produzir distorção perceptível que prejudique a conversa.
2. Cobrir fala baixa e normal, teclado, ventilador, headset e microfone de notebook. Gravações reais complementam os testes sintéticos do gate.
3. Medir CPU, memória e latência adicional nas plataformas homologadas, inclusive durante compartilhamento de tela. Registrar a configuração do equipamento e comparar com o processamento desligado.
4. Ligar/desligar o filtro e trocar de dispositivo durante a chamada não interrompe permanentemente a captura nem exige reiniciar o aplicativo.
5. A combinação filtro + alvo de −60 dBFS passa pelos critérios de F01; testar o filtro isoladamente não basta.

**Decisões para implementação:** escolher o mecanismo de supressão após avaliar qualidade, custo, licença e empacotamento. Rever as decisões históricas de DSP citadas em `ganho.rs` à luz do novo escopo. Este documento não escolhe biblioteca nem promete equivalência técnica com o Discord.

## Entrega conjunta

F01 e F02 formam um único pacote de qualidade de voz para a v15. Primeiro, estabelecer gravações e medidas de referência; depois integrar a supressão, ajustar ativação e sustentação, expor os controles e validar a chamada nativa entre máquinas.

O filtro trata o **microfone**. O retorno dos participantes pelo áudio do compartilhamento de tela permanece no escopo da review: exige separar o áudio capturado do áudio da conversa. Cancelamento de eco acústico também exige avaliação própria e não está implicitamente resolvido pela supressão de ruído.
