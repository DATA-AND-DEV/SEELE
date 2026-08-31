# 09 — Roadmap

Milestones sequenciais. Cada um termina com algo demonstrável e critérios de aceite verificáveis. **Não avançar com critério pendente.**

---

## M0 — Fundação e decisões

Resolver as decisões em aberto que travam tudo e montar o esqueleto.

- Workspace de crates conforme `01`, compilando vazio.
- CI: build, `clippy -D warnings`, `fmt --check`, testes, nas três plataformas.
- **Decidir e registrar:** formato de serialização (`02`), estratégia de certificado (`08`), mecanismo de autenticação (`08`).
- Um ADR curto por decisão, em `docs/adr/`.

**Aceite:** `cargo build` e `cargo test` verdes nos três SOs. Decisões registradas.

---

## M1 — Prova de conceito de áudio ⚠️

**O milestone de maior risco. Vem cedo por isso.** Nada de protocolo ou interface aqui.

- Captura via `cpal` → Opus → UDP simples → decode → playback, entre duas máquinas.
- Jitter buffer adaptativo como módulo puro e determinístico, com testes contra padrões de rede sintéticos.
- Medição real de latência boca-a-ouvido.
- Validado em WASAPI, CoreAudio e PipeWire.

**Aceite:** duas máquinas em LAN conversam com áudio inteligível, latência medida
abaixo de **70 ms**, sem estalo em 10 minutos contínuos. **Perda induzida de 5%
em rajada** permanece inteligível.

Os dois números foram precisados depois de medir:

- **70 ms**, não 60. Ver a nota em `00-visao-geral.md` e o orçamento por estágio
  em `docs/adr/0009-orcamento-de-latencia.md`.
- **Em rajada**, não independente. Perda real vem em corridas, e o modelo
  Gilbert-Elliott de `seele_audio::netsim` com rajada média de 4 quadros produz
  buracos de até 480 ms — enquanto o PLC do Opus cobre 20 ms. "Inteligível com
  5% de perda" portanto significa: inteligível **apesar de** interrupções de meio
  segundo. É o teste mais duro e o que se parece com Wi-Fi e rede móvel reais.

Se este milestone escorregar muito, o escopo do projeto precisa ser revisto — não os outros milestones.

---

## M2 — Protocolo e servidor mínimo

- `seele-proto` com tipos, serialização e versionamento.
- Conexão QUIC via `quinn`, handshake completo, PADRÃO LARANJA → AZUL.
- `seeled` com uma sala de voz fixo, encaminhamento de datagrams, sem persistência.
- Cliente de linha de comando feio, sem TUI, só para exercitar o protocolo.

**Aceite:** três clientes entram no mesmo VoiceRoom e conversam por voz através do servidor. Cliente sem permissão é rejeitado. Fuzzing do parser sem crash.

---

## M3 — Estado, texto e persistência

- PERSISTENCE: SQLite, migrações, VoiceRooms e Linhas configuráveis.
- PERMISSIONS: contas, papéis, permissões, banimento.
- Mensagens de texto com histórico paginado por cursor.
- Telemetria e cálculo do sinal.
- Keepalive, reconexão e a janela de 5 minutos da bateria interna.

**Aceite:** servidor reiniciado preserva estado e histórico. Queda de rede de 60 s é recuperada de forma transparente. Matriz de permissões coberta por testes.

---

## M4 — TUI

O primeiro milestone com o produto de verdade.

- Layout completo conforme `05`.
- Modos Normal, Inserção, Comando e Busca.
- Push-to-talk, VAD, mudo, volume por pessoa.
- Todos os seis estados visuais, incluindo boot e bateria interna.
- Tema aplicado conforme `07`, com degradação para 256 e 16 cores e modo sem cor.

**Aceite:** utilizável por SSH em 80×24 e 16 cores sem perda de informação. RSS abaixo de 60 MB. Boot até pronto para falar em menos de 1,5 s. Uma pessoa de fora do projeto consegue conectar e conversar só com `?`.

**Este é o ponto de release pública alfa.**

---

## M5 — Cliente desktop

- Tauri sobre `seele-ffi`.
- Implementação do design entregue pelo Claude Design.
- Paridade funcional com a TUI.
- Instaladores para os três SOs.

**Aceite:** binário abaixo de 30 MB, inicialização abaixo de 2 s, mesma sessão retomável entre TUI e app.

---

## M6 — Mobile (consumo)

- Protótipos descartáveis de áudio em background nas plataformas candidatas; **só então** decidir Flutter vs nativo (`06`).
- Ouvir, falar, ler, responder. Nada de administração.
- Sobrevive a bloqueio de tela, chamada recebida e troca de rede.

**Aceite:** 30 minutos em VoiceRoom com a tela bloqueada, sem queda, com consumo de bateria documentado.

---

## Pós-v1 (não planejar agora)

E2EE de mídia, áudio espacial, federação, gravação de sessão, vídeo. Listado apenas para lembrar que **não** está em v1.
