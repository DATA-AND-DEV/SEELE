# Microfone Safari Simulator — investigação de 28/10/2026

**Estabilidade de 30 s: FALHOU. O controle contemporâneo reproduziu a perda sem o motor SEELE; a configuração/causa interna do Safari Simulator ainda está em investigação.** Nenhum Xcode ou áudio físico foi iniciado por esta investigação; as execuções são controladas pela rodada principal. `voz-web.js` não foi alterado nesta investigação.

## Resultado atual

Fonte: `/private/tmp/seele-018/xcode-microfone.log`, teste `SafariMicrofoneUITests/testMicrofoneLiveTresEncerramentosENadaReabreSozinho`.

| Tempo da página | Evidência |
| --- | --- |
| 1644 ms | Primeira tentativa getUserMedia não subiu; suas faixas foram paradas. Evento `falha:microfone`. |
| 1725 ms | Segunda tentativa getUserMedia. |
| 2036 ms | `audio-ligado`; uma faixa viva, contexto running. |
| 21740 ms | `captura:encerrada`, seguido de `falha:microfone`. Nenhum pagehide. |
| 21741–21744 ms | `stop()` da limpeza do motor. |
| 21744–21750 ms | `audio-fechado` e contexto closed. |

O teste registrou queda na amostra de 19 s após a abertura e falhou. O encerramento da faixa antecede os `stop()` da limpeza: esses registros não demonstram que o motor iniciou a queda. O handler de `voz-web.js:803` recebe `ended` da faixa atual, anuncia perda e fecha o áudio; não reabre captura sozinho. Isso é coerente com o contrato de liberação após perda. Não prova captura estável nem recepção de amostras antes da perda.

## Comparação histórica e limite do controle atual

O resultado histórico real `SEELE-RELEASES/preparacao-0.17/xcode/microfone-cru-sem-processamento-45s.xcresult`, lido com `xcresulttool get test-results tests`, registra no iOS Simulator 27:

- getUserMedia cru, sem motor SEELE, com eco/ruído/ganho desativados;
- evento ended do navegador em 21297 ms;
- `stop(): 0`;
- 0/188 leituras com sinal, pico 0, AudioContext running;
- teste de 45 s FALHOU.

Essa reprodução sem motor, e o relato histórico de “no audio received in 20 seconds”, tornam a limitação de captura do Simulator uma hipótese forte para o resultado atual. São evidências históricas, não confirmação causal desta rodada.

O controle contemporâneo `/private/tmp/seele-018/xcode-microfone-cru-controle.log` executou **um** teste e passou. O método `testDiagnosticoMicrofoneCruSemProcessamento` mantém a faixa por **15 s**, sem medir amostras. Portanto não atravessa o intervalo de queda observado aos 19–21 s. A tentativa anterior com classe inexistente executou zero testes e não conta como evidência.

O controle longo contemporâneo foi executado pela rodada principal e registrado em `/private/tmp/seele-018/xcode-microfone-controle45-lifecycle3.log`:

| Teste | Resultado real |
| --- | --- |
| `testDiagnosticoMicrofoneCruSemProcessamentoLongo` | **FALHOU**, 60,734 s de execução total. Retenção solicitada 45 s; ended do navegador em 22048 ms, `stop(): 0`, 0/188 leituras com sinal, pico 0, contexto running. |
| `testMicrofoneLiveTresEncerramentosENadaReabreSozinho`, retenção configurada de **3 s** | **PASS**, 47,732 s de execução total. Os três encerramentos e a ausência de reabertura automática foram exercitados. |

A rodada executou dois testes e teve uma falha; não é uma rodada integralmente aprovada. O controle cru longo reproduz a perda sem o SEELE nesta rodada, isolando-a da implementação do motor. Ainda falta distinguir a configuração de entrada de áudio do Simulator de uma limitação interna do Safari/Simulator. A prova curta de liberação não atravessa o limiar da perda e não substitui estabilidade. Nenhuma estabilidade de captura física foi validada.

## Erros corrigidos somente no diagnóstico da fixture

Arquivo: `SEELE-RELEASES/preparacao-0.17/xcode/fixture/microfone.js`.

1. `quemEncerrou` comparava stops de **todas** as faixas. O stop da primeira tentativa aos 1644 ms suprimia o rótulo `ended-sem-stop` da segunda faixa, embora esta não tivesse sido parada. IDs por faixa agora acompanham stops e ended; parada posterior à perda não apaga a causa.
2. `falhasDoMotor` lia `voz.contas().falhas`. `contas()` é assíncrono e retorna `{pagina:{falhas}}`; a expressão anterior exibia zero mesmo com eventos de falha no log. O diagnóstico agora usa os eventos `falha` realmente emitidos pelo motor, sem pedir contas a worklet possivelmente fechado.

Teste puro da fixture, sem Safari ou captura: `xcode/fixture/microfone-observadores.test.cjs` **PASS**. Cobre primeira faixa parada + segunda encerrada pelo navegador + limpeza posterior + parada própria de terceira faixa + exibição de falha emitida. Ele valida o observador, não o motor nem a estabilidade.

## Conclusão permitida neste ponto

A limpeza após o evento ended está demonstrada. A perda também ocorreu no controle contemporâneo sem motor SEELE, sem stop e sem amostras com sinal; por isso não há evidência de um timer do motor causando a queda. A captura Safari/ambiente do Simulator é o ponto isolado, com configuração de entrada ainda em verificação pela rodada principal. O gate de estabilidade continua reprovado. O cenário de liberação com retenção de 3 s passou separadamente; não se reduziu o tempo do teste forte, não se adicionou recuperação automática e não se declarou homologação de microfone físico.
