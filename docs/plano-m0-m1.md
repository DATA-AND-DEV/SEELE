# Plano de implementação — M0 e M1

> Documento de planejamento. Fonte de verdade dos requisitos continua sendo `specs/`.
> Onde este plano diverge das specs, a divergência está marcada e justificada.
> Escrito antes de qualquer linha de código de produção existir.

**Status:** aprovado para execução.
**Escopo:** M0 (fundação e decisões) e M1 (prova de conceito de áudio), conforme `specs/09-roadmap.md`.
**Base de design:** `design/` — ver seção 6.

---

## Índice

1. [Como ler este plano](#1-como-ler-este-plano)
2. [M0 — Fundação e decisões](#2-m0--fundação-e-decisões)
3. [M1 — Prova de conceito de áudio](#3-m1--prova-de-conceito-de-áudio)
4. [Decisões em aberto](#4-decisões-em-aberto)
5. [Riscos técnicos](#5-riscos-técnicos)
6. [Base de design](#6-base-de-design)
7. [Contradições e lacunas nas specs](#7-contradições-e-lacunas-nas-specs)

---

## 1. Como ler este plano

### Escala de esforço

Pontos relativos `1 · 2 · 3 · 5 · 8`. **Não são dias** — são tamanhos comparáveis entre si.
Total M0 ≈ 26, M1 ≈ 61. A proporção é intencional e reflete onde o risco está concentrado.

### Destino do código

| Marca | Significado |
|---|---|
| `SOBREVIVE` | Vai para os crates definitivos. Escrito com o padrão de qualidade de `specs/10-convencoes.md` |
| `DESCARTÁVEL` | Prova de conceito. Morre no fim do milestone. Não passa por revisão de arquitetura |
| `FERRAMENTA` | Não é produto, mas fica no repositório: rigs de teste, simuladores, checklists |

A distinção é operacional: código `DESCARTÁVEL` pode ter `unwrap()`, pode ignorar i18n e pode viver em `examples/` ou `xtask/`. Código `SOBREVIVE` não pode.

### Política de dúvidas

Conforme combinado: **não bloqueio o início esperando respostas.** Cada decisão em aberto da seção 4 tem um ponto no plano onde ela vence e uma recomendação minha. Ao chegar nesse ponto:

1. Levanto a pergunta com o contexto já concreto (código na mão, não hipótese).
2. Se não houver resposta, **sigo com a recomendação** e registro o ADR com status `aceito por default`.
3. ADR marcado assim é explicitamente revisável sem custo de reputação — mas o custo técnico de reverter cresce com o tempo, e esse custo está estimado em cada decisão.

Exceção: as decisões marcadas **PARAR** na seção 4 não têm default seguro. Nessas, eu paro e pergunto.

### Sobre a ordem: áudio continua antes do protocolo

`specs/09-roadmap.md` coloca M1 (áudio) antes de M2 (protocolo e servidor) deliberadamente. **Este plano mantém essa ordem.**

Faço **uma** realocação, pequena: a compilação do libopus nas três plataformas sobe para M0 (tarefa `M0.4`). Justificativa: isso é problema de *build system*, não de áudio. Descobrir na semana 2 de M1 que o toolchain C não fecha no Windows queima tempo do milestone crítico com um problema que não ensina nada sobre áudio. M0 já monta CI nos três SOs; o custo marginal é um teste de round-trip encode/decode de trinta linhas.

Tudo o mais de M1 permanece em M1.

---

## 2. M0 — Fundação e decisões

| ID | Tarefa | Depende de | Esforço | Destino |
|---|---|---|---|---|
| **M0.1** | Workspace: `git init`, `Cargo.toml` raiz, crates de `01` como stubs, `rust-toolchain.toml` com MSRV fixado, `.gitignore` | — | 2 | SOBREVIVE |
| **M0.2** | Baseline de lint: `rustfmt` padrão, `clippy -D warnings`, `#![forbid(unsafe_code)]` em todos os crates menos `seele-ffi` e o binding de áudio, `deny.toml`, `cargo audit` | M0.1 | 2 | SOBREVIVE |
| **M0.3** | CI matriz macOS · Windows · Linux: build, fmt, clippy, test. Runners pinados, cache de dependências | M0.2 | 3 | SOBREVIVE |
| **M0.4** | **Deps nativas no CI** — libopus vendorizado/estático nos três SOs, `libasound2-dev` no Linux, + teste de round-trip Opus provando o toolchain. *Realocado de M1* | M0.3 | 5 | SOBREVIVE |
| **M0.5** | Guarda da regra de dependência de `01`: step de CI que falha se `proto` depender de alguém, se `audio` depender de `core`, etc. | M0.1 | 1 | SOBREVIVE |
| **M0.6** | Infra de ADR: `docs/adr/0000-template.md` + índice, no formato de `10` | — | 1 | SOBREVIVE |
| **M0.7** | **Glossário bilíngue normativo completo** — termo em inglês para cada linha de `07`. Bloqueia nomes de tipo, de módulo e de variante de erro. *Ver lacuna G3* | M0.6 | 2 | SOBREVIVE |
| **M0.8** | ADRs das decisões travantes: serialização · certificados · autenticação (direção) · porta · política de DSP · orçamento de latência · binding Opus | M0.6 | 3 | SOBREVIVE |
| **M0.9** | ADR de mecanismo de i18n + fronteira `enum de erro → texto` estabelecida em `seele-proto`. *Ver lacuna G4* | M0.6 | 2 | SOBREVIVE |
| **M0.10** | Esqueletos de teste: `proptest`, `criterion`, alvo `cargo-fuzz` placeholder em `seele-proto` | M0.3 | 2 | SOBREVIVE |
| **M0.11** | **Rigs de teste**: segunda máquina em LAN, caixa Windows, caixa Linux com PipeWire, dois headsets, cabo de loopback. Checklist de plataforma versionado | — | 3 | FERRAMENTA |
| **M0.12** | **Congelar tokens de design**: reconciliar `seele-tokens.*` com o protótipo v2, recalcular mapeamento ANSI 256/16, verificar contraste. *Ver seção 6* | M0.7 | 2 | SOBREVIVE |

**Comece por `M0.11`.** É a única tarefa com prazo de entrega fora do seu controle e é pré-requisito duro do aceite de M1. Nenhuma spec a menciona.

### Ordem de execução sugerida

```
M0.11 ──────────────────────────────────────────▶ (paralelo, prazo externo)

M0.1 ──▶ M0.2 ──▶ M0.3 ──▶ M0.4 ──▶ M0.10
   └────▶ M0.5

M0.6 ──┬─▶ M0.7 ──▶ M0.12
       ├─▶ M0.8
       └─▶ M0.9
```

### Aceite de M0

Conforme `09`, mais o que este plano acrescenta:

- [ ] `cargo build` e `cargo test` verdes nos três SOs
- [ ] ADRs escritos para todas as decisões de M0 (inclusive as `aceito por default`)
- [ ] **libopus compilando e linkando estaticamente nos três SOs**
- [ ] **Glossário bilíngue fechado**
- [ ] **Arquivo único de tokens de design, sem palheta concorrente no repositório**

### Nota de escopo: não criar crates vazios

`01` descreve seis crates. Em M0 crie apenas `seele-proto`, `seele-audio` e o stub de `seele-core`. Os outros três nascem quando o primeiro código deles existir. Crate vazio custa tempo de CI em toda build e convida a colocar código no lugar errado por já haver uma gaveta esperando. Ver risco R10.

---

## 3. M1 — Prova de conceito de áudio

> `09`: *"O milestone de maior risco. Vem cedo por isso."*
> *"Se este milestone escorregar muito, o escopo do projeto precisa ser revisto — não os outros milestones."*

Ordem pensada para que o experimento mais barato mate a hipótese mais cara primeiro.

| ID | Tarefa | Depende de | Esforço | Destino |
|---|---|---|---|---|
| **M1.1** | **Spike-portão:** medir latência de ida-e-volta do dispositivo em macOS com `cpal`, sem codec e sem rede. Só device → device | M0.4 | 2 | DESCARTÁVEL |
| **M1.2** | Rig de medição de latência: clique + correlação cruzada, acústico e por cabo. Método documentado | M1.1 | 3 | FERRAMENTA |
| **M1.3** | Caminho tempo-real: callback `cpal` → `rtrb` → thread de processamento, nos dois sentidos. Portão `assert_no_alloc` no CI | M1.1 | 5 | SOBREVIVE |
| **M1.4** | Conversão de taxa e formato nos **dois** sentidos (device ↔ 48 kHz mono f32). *A spec só descreve na captura — lacuna G7* | M1.3 | 3 | SOBREVIVE |
| **M1.5** | Wrapper do codec Opus: VOIP, 20 ms, DTX, FEC in-band, bitrate mutável em tempo de execução | M0.4 | 3 | SOBREVIVE |
| **M1.6** | Simulador de degradação de rede: perda em rajada, jitter, reordenação, duplicação, atraso | — | 3 | FERRAMENTA |
| **M1.7** | **Jitter buffer** como módulo puro e determinístico + testes de propriedade. O centro do milestone | M1.6 | 8 | SOBREVIVE |
| **M1.8** | Compensação de deriva de clock entre dispositivos de captura e reprodução. *Ausente de todas as specs — risco R3* | M1.7 | 5 | SOBREVIVE |
| **M1.9** | Distinção entre silêncio DTX/VAD e perda real; política de PLC (1 quadro) → silêncio com fade. *Lacuna G5* | M1.7 | 3 | SOBREVIVE |
| **M1.10** | Struct de métricas de `03` como dado puro, sem referência a interface | M1.7 | 1 | SOBREVIVE |
| **M1.11** | Transporte UDP simples + harness de dois processos | M1.5 | 2 | DESCARTÁVEL |
| **M1.12** | Mixer: N decoders → soma + ganho por usuário + clipping suave | M1.5 | 3 | SOBREVIVE |
| **M1.13** | PTT e VAD com histerese + hangover 300 ms, alimentando um único sinal `speaking: bool` | M1.3 | 3 | SOBREVIVE (o gating; o tratamento de tecla é descartável) |
| **M1.14** | Máquina de estados de troca de dispositivo a quente: pausa → reenumera → retoma. *`03` exige; `09` não menciona em M1* | M1.3 | 5 | SOBREVIVE |
| **M1.15** | Passes de validação: CoreAudio, WASAPI, ALSA + PipeWire. Checklist preenchido por plataforma | M0.11, M1.11 | 8 | FERRAMENTA (docs) |
| **M1.16** | Aceite: soak de 10 min sem estalo · perda induzida de 5% inteligível · relatório de latência boca-a-ouvido | M1.15 | 3 | — |
| **M1.17** | Retro: atualizar `00` e `03` onde a realidade divergiu. `10` exige isso | M1.16 | 1 | SOBREVIVE |

### Portão em M1.1 — PARAR e perguntar

Se o round-trip **só de dispositivo** já passar de ~35 ms com buffers padrão, pare antes de escrever o jitter buffer. Isso significa que os 60 ms de `00` exigem configuração de buffer de baixa latência por plataforma, o que muda o escopo de M1 e possivelmente o alvo de `00`. Ver decisão D2 e contradição C1.

### Duas ressalvas de escopo

**Bitrate adaptativo não fecha em M1.** `03` especifica reação à taxa de perda, mas isso exige que o receptor realimente o emissor, e M1 não tem canal de controle. Recomendação: em M1 provar apenas que o encoder muda de bitrate a quente (parte de `M1.5`) e deixar a *política* de adaptação para M2, quando existir protocolo.

**O mixer entra em M1 mesmo sem `09` pedir.** `09` exige duas máquinas em M1; `03` especifica mixer. Incluo `M1.12` porque o aceite de M2 pede três clientes conversando, e construir mixer dentro de M2 seria empurrar código de áudio para o milestone de protocolo — exatamente o que a ordem do roadmap quer evitar.

### Como M1 protege a fronteira `seele-core` ↔ cascas

Não é lembrete; são regras verificáveis nas tarefas acima:

- `M1.7` e `M1.12` vivem em `seele-audio`, que por `01` só pode depender de `seele-proto`. O guarda de CI (`M0.5`) transforma a regra em build vermelho.
- `M1.10` entrega métricas como **dado puro**. Nenhuma formatação, nenhuma cor, nenhuma faixa `nominal/degradado` — as faixas de `07` são decisão de casca. Se aparecer um `to_string()` temático dentro de `seele-audio`, é erro de arquitetura, não detalhe.
- `M1.13` emite `speaking: bool`. Qual tecla aciona PTT não é assunto de `seele-audio`.

### Como o tema entra já em M0/M1

O tema é vocabulário de produto, então ele custa esforço agora, não em M4:

- `M0.7` fecha o glossário nas duas línguas **antes** de qualquer tipo ser nomeado.
- Crates e binários já nascem com os nomes canônicos: `seeled`, `plug`, `seele-*`.
- Variantes de erro em `seele-proto` nascem temáticas (`BluePatternNotEstablished`, não `AuthFailed`) e nascem como `enum` — nunca `String` — porque `02` diz que a casca decide a apresentação e `10` exige i18n desde o início. Renomear isso depois de M2 custa uma versão de protocolo.

### Aceite de M1

Conforme `09`:

- [ ] Duas máquinas em LAN conversam com áudio inteligível
- [ ] Latência medida abaixo de 60 ms *(ver C1 — este critério precisa ser reescrito antes de ser cobrado)*
- [ ] Sem estalo em 10 minutos contínuos
- [ ] Perda induzida de 5% permanece inteligível
- [ ] Validado em WASAPI, CoreAudio e PipeWire

---

## 4. Decisões em aberto

Todas as marcações `[EM ABERTO]` das specs, mais o que falta e não está marcado.

Coluna **Vence em** = ponto do plano onde eu levanto a pergunta.
Coluna **Custo de reverter** = quanto custa mudar de ideia depois desse ponto.

### 4.1 Decisões que vencem em M0/M1

| # | Decisão | Vence em | Recomendação | Custo de reverter |
|---|---|---|---|---|
| **D1** | AEC / AGC / supressão de ruído | M1.3 | Fone obrigatório, zero DSP em C++ | Baixo se o seam existir |
| **D2** | Orçamento de latência | M1.1 · **PARAR** | Tabela por estágio; reescrever aceite | Alto — é critério de aceite |
| **D3** | FEC in-band do Opus | M1.7 | Desligado em M1, reavaliar em M2 | Médio — muda o jitter buffer |
| **D4** | Rigs de teste | M0.11 · **PARAR** | Máquinas físicas, não VM | Bloqueante |
| **D5** | Binding Opus | M0.4 | `audiopus` + libopus estático | Baixo |
| **D6** | Serialização | M0.8 | `postcard` isolado em um módulo | Baixo por desenho |
| **D7** | Certificados | M0.8 | TOFU padrão, ACME opcional | Médio |
| **D8** | Autenticação | M0.8 | Ed25519 + convite de uso único | Alto após M2 |
| **D9** | Postura de direitos | M0.1 | Repositório privado até M4 | Alto se público |
| **D10** | Porta | M0.8 | Confirmar `8383` — **mas ver C9** | Baixo |
| **D11** | Palheta canônica do design | M0.12 | Adotar v2, regenerar tokens | Baixo agora, médio em M4 |

---

**D1 — Cancelamento de eco, AGC e supressão de ruído.**
`03` marca os três separadamente; trato como uma decisão só, porque as três vivem na mesma biblioteca.

> **Recomendação:** opção 1 de `03` — fone de ouvido obrigatório e documentado. **Nenhuma dependência de DSP em C++ em v1**, com a arquitetura deixando pronto o seam de feature de compilação para `webrtc-audio-processing` depois.
>
> **Motivo:** essas três caixas são exatamente os três `[EM ABERTO]` do diagrama de pipeline de `03` — não dá para escrever o pipeline de M1 sem elas resolvidas. E puxar um build C++ multiplataforma para dentro de M1 dobra o risco do milestone mais arriscado do projeto, para resolver um problema que o público-alvo já resolve com hardware. A própria `03` recomenda isso; o que este plano acrescenta é que a decisão vence **antes** de M1, não durante.

**D2 — Orçamento de latência. Os 60 ms são portão ou alvo?**

> **Recomendação:** converter em tabela de orçamento por estágio e redefinir o aceite de M1 como *"< 60 ms com o jitter buffer no piso de 20 ms em LAN limpa; < 90 ms com o alvo padrão de 40 ms"*.
>
> **Motivo:** a conta não fecha como está escrita. 20 ms de acumulação de quadro + 40 ms de alvo inicial de jitter (`03`) já são 60 ms **antes** de buffer de captura, encode, rede, decode e buffer de reprodução — realisticamente ~80 ms. `00` e `03` se contradizem numericamente e `09` transforma o número em critério verificável. Sem essa resposta, M1 falha o aceite por aritmética e o tempo vai embora perseguindo um número inalcançável. Ver C1.
>
> **PARAR:** não há default seguro aqui, porque o default é "M1 reprova".

**D3 — FEC in-band do Opus.**

> **Recomendação:** manter DTX ligado, deixar o FEC in-band desligado em M1, reavaliar em M2 com dados de perda reais.
>
> **Motivo:** `03` já lista FEC como `Ligado` na tabela de parâmetros, enquanto `02` pergunta em `[EM ABERTO]` se FEC entra em v1 — a spec se contradiz consigo mesma (C4). Mais importante: FEC in-band (LBRR) só serve se o decoder receber o pacote *seguinte* ao perdido, o que obriga o jitter buffer a segurar um quadro extra: **+20 ms**. Colide de frente com D2. É input de projeto do jitter buffer, então vence em `M1.7`.

**D4 — Rigs de teste. Que hardware existe hoje?**

> **Recomendação:** no mínimo uma caixa Linux física com PipeWire e uma caixa Windows física, mais uma segunda máquina qualquer na LAN. **VM não serve.**
>
> **Motivo:** o aceite de M1 exige validação em WASAPI, CoreAudio e PipeWire e duas máquinas em LAN. Runners de CI não têm dispositivo de áudio, e áudio em VM tem latência e comportamento de driver que não representam nada. Nenhuma spec menciona isso e é a coisa com maior prazo de aquisição no plano inteiro.
>
> **PARAR:** é bloqueante e não tem contorno técnico.

**D5 — Binding do Opus: `audiopus` vs `opus`, sistema vs estático.**

> **Recomendação:** `audiopus` com libopus vendorizado e linkado estaticamente.
>
> **Motivo:** `00` promete *"um binário"*. Depender de libopus do sistema quebra a promessa e transforma cada plataforma de destino em problema de empacotamento.

**D6 — Serialização: `postcard` vs `prost`.**

> **Recomendação:** `postcard`, com todo o encoding confinado a um módulo de `seele-proto` e os tipos de domínio sem nenhum atributo específico do formato.
>
> **Motivo:** concordo com `02`. Clientes de terceiros são não-objetivo declarado em `00`; pagar boilerplate de `.proto` por um objetivo que a spec explicitamente rejeita é caro. O isolamento torna a troca mecânica se a premissa mudar.

**D7 — Certificados: TOFU vs ACME.**

> **Recomendação:** TOFU com pinning por padrão; ACME como opção documentada.
>
> **Motivo:** ACME exige domínio e portas 80/443, o que contradiz a simplicidade de "uma porta UDP" de `01` e o perfil de operador de `08` — alguém confiável mas não especialista. O modelo do SSH é o que o público-alvo já tem na cabeça. Aviso de troca de chave como `Alerta · 警告` bloqueante, como `08` já pede.

**D8 — Autenticação. Aqui há contradição de cronograma.**

> **Recomendação:** fechar a *direção* agora (Ed25519 + token de convite de uso único; senha como fallback opcional do operador) e implementar em M2/M3.
>
> **Motivo:** `09` coloca "mecanismo de autenticação" no aceite de M0; `08` diz "escolher em M2" (C2). A direção precisa vencer em M0 porque determina se `seele-proto` carrega o formato desafio-resposta e se o schema de CASPER tem colunas de senha — decidir depois força bump de versão de protocolo. A *implementação* pode esperar M2.
>
> **Sinal de apoio:** o protótipo de design já assume esse caminho — a tela `02 AUTENTICAÇÃO` tem campo `ed25519-0x8F41C2`.
>
> **Atenção:** essa escolha tem conflito não resolvido com o aceite de M5. Ver C3.

**D9 — Postura de direitos / repositório público.**

> **Recomendação:** repositório privado até M4. Naming original, sem logos NERV/SEELE, sem arte, sem trilha, sem nome de personagem como marca.
>
> **Motivo:** privado até M4 tira a decisão do caminho crítico sem custo. Quando ela vencer, note que o risco não é uniforme:
> - `MELCHIOR` / `BALTHASAR` / `CASPER` são os nomes bíblicos dos três magos — seguros.
> - `Cage`, `Piloto`, `Dogma Central`, `Linha` são vocabulário genérico — baixo risco.
> - A exposição real se concentra em **`A.T. Field`** e **`Entry Plug`**, que são cunhagens da franquia. Se o projeto for público e você quiser folga, esses dois são os que têm sinônimo funcional.
>
> **Ação imediata, independente da decisão:** `design/uploads/pasted-1786132126320-0.png` é um frame do anime, usado como referência de mood no Claude Design. Não pode ser distribuído com o produto nem sobreviver num repositório público. Ver seção 6.

**D10 — Porta.**

> **Recomendação:** confirmar `8383` e registrar no ADR. **Mas o protótipo de design usa `7743`** — ver C9. Uma das duas fontes precisa ceder antes de a porta aparecer em documentação de usuário.

**D11 — Palheta canônica do design.** Detalhada na seção 6.

### 4.2 Decisões que podem esperar

| # | Decisão | Fonte | Vence em | Recomendação |
|---|---|---|---|---|
| D12 | IPv6 / NAT traversal | `01` | M2 | IP público, escuta dual-stack, sem NAT traversal nem relay. É o modelo de VPS que `00` descreve |
| D13 | Política acima de 20 falantes (N ativos) | `01`, `04` | M3 | Encaminhar os N mais recentes a falar, medido **no servidor** pela taxa de datagrams, não por energia reportada pelo cliente — `04` já reconhece que energia do cliente é pouco confiável |
| D14 | Limite de mensagem / anexos | `02` | M3 | **Revisada.** O teto de corpo em 4 KiB fica, e foi fixado já em M0 porque `08` exige limite de tamanho antes de alocar. «Sem anexos em v1» caiu: o ADR 0027 os construiu com teto total fixo e despejo do mais antigo |
| D15 | Endpoint de saúde | `04` | M3 | HTTP mínimo separado, atrás de bind configurável. Métricas Prometheus já são exigidas por `04`, então a superfície HTTP existe de qualquer jeito — não construa duas |
| D16 | Recarga a quente de config | `04` | M3 | Não em v1, como `04` já suspeita |
| D17 | Compressão de histórico | `02` | M3 | Não em v1. Não otimizar antes de medir (`10`) |
| D18 | PTT global sem foco | `03` | M4 | Exigir foco em v1. Permissão de acessibilidade no macOS mata a instalação em cinco minutos que `10` promete no README |
| D19 | Espaço como PTT vs digitação | `05` | M4 | PTT só no modo Normal (a proposta já em `05`), tecla reconfigurável, VAD como alternativa anunciada na ajuda |
| D20 | Leitor de tela | `05` | M4 | Investigar, não prometer. O ganho real está no modo sem cor, que já é requisito |
| D21 | Framework do frontend desktop | `06` | M5 | Decidir com o design em mãos. Interface densa e pouco interativa favorece Solid ou HTML/TS puro. **O protótipo entregue é HTML/CSS sem framework — isso é evidência a favor** |
| D22 | Plataforma mobile | `06` | M6 | Manter como está: protótipos descartáveis primeiro, decisão depois |

---

## 5. Riscos técnicos

Ordenados por **probabilidade × impacto no prazo**, não por dificuldade intrínseca.
Onde discordo da priorização de `00`, está dito.

### R1 — Logística de validação em três plataformas

`00` cita divergência de `cpal` entre backends. Concordo com o problema, **discordo do enquadramento**: o gargalo não é o código, é o acesso a três máquinas físicas com áudio real. PipeWire varia por distro, por versão e por configuração de sessão; "testar em PipeWire" não é um teste, é uma matriz. Nenhum CI cobre isso.

Maior probabilidade da lista e o único cujo prazo não depende de você.
**Mitigação:** `M0.11` primeiro; checklist versionado por plataforma.

### R2 — O orçamento de 60 ms não fecha com os defaults

Não está listado em spec nenhuma. Segundo lugar porque é **portão de aceite** de M1, e `09` diz que se M1 escorregar muito o escopo do *projeto* é revisto — ou seja, esse risco não atrasa um milestone, atrasa o produto por definição.
**Mitigação:** portão `M1.1` antes de qualquer código de jitter buffer; decisão D2.

### R3 — Deriva de clock entre dispositivos de captura e reprodução

Ausente de todas as specs. Dispositivos independentes rodam a 48 kHz nominais e ~48.001 reais; ao longo de minutos o jitter buffer esvazia ou transborda. Aparece **exatamente** no soak de 10 minutos que `09` exige em M1, e se manifesta como estalo intermitente — o sintoma mais caro de diagnosticar que existe em áudio.

Alto na lista porque a probabilidade é próxima de 1 e a spec não o antecipa.
**Mitigação:** `M1.8`, projetado junto com o jitter buffer, não depois.

### R4 — Toolchain C/C++ nas três plataformas

Mundano, alta probabilidade, come dias. Piora muito se D1 for na direção de `webrtc-audio-processing`.
**Mitigação:** `M0.4` — resolver antes de M1 começar.

### R5 — DTX/VAD corrompendo a contabilidade de perda

Com DTX e VAD ligados, o fluxo simplesmente para em silêncio. Um jitter buffer ingênuo conta cada silêncio como perda; a penalidade de perda vale até 30 pontos na Taxa de Sincronização, que é *a* métrica assinatura de `07`. Resultado: todo mundo aparece degradado quando ninguém está falando.

Probabilidade média, visibilidade máxima — quebra o elemento mais visível do produto.
**Mitigação:** `M1.9`.

### R6 — Cancelamento de eco

`00` ranqueia como terceiro risco. **Discordo da priorização.** Aceita a recomendação de fone obrigatório — que a própria `03` já faz — isso deixa de ser risco de engenharia e vira tarefa de documentação. Só volta ao topo se D1 for rejeitada.

É um risco **condicional**, não ativo. Mantê-lo alto na lista distorce onde a atenção deve ir.

### R7 — Erosão da disciplina de tempo real

Uma alocação que entra no callback do `cpal` seis meses depois produz estalo que ninguém liga à causa. Probabilidade média ao longo do tempo, mitigação barata: o portão `assert_no_alloc` no CI (`M1.3`) que `03` já sugere. Barato agora, caro de retrofitar.

### R8 — Erosão da fronteira núcleo/casca

Probabilidade **baixa em M0/M1**, porque nenhuma casca existe ainda — mas o código que sobrevive de M1 é justamente o que será consumido por elas. O risco real é escrever `seele-audio` já assumindo um consumidor.
**Mitigação:** `M1.10` como dado puro; `M0.5` como guarda automático.

### R9 — Escopo do tema

`00` lista como quarto risco. Concordo que é real, mas é risco de **M4 em diante**. Em M0/M1 não há uma tela sequer; o único custo temático é `M0.7` + `M0.12`, que é fechar vocabulário e palheta — isso é economia, não gasto.

### R10 — Workspace grande demais para código zero

Seis crates e dois diretórios de app antes de existir uma linha de lógica. Baixo, mas custa tempo de CI em toda build e convida a colocar código no crate errado por já haver um lugar vazio esperando.
**Mitigação:** criar três crates em M0, não seis.

---

## 6. Base de design

O arquivo `Aguardando respostas.zip` foi extraído em `design/`. **Isto resolve a lacuna G11** — `06` e `09` referenciavam um `PROMPT-CLAUDE-DESIGN.md` que não existia na pasta `specs/`.

### O que veio

| Arquivo | Conteúdo |
|---|---|
| `seele-tokens.json` / `.css` | Tokens de cor, tipografia, espaçamento, borda, movimento. **Inclui mapeamento ANSI 256 e ANSI 16** |
| `Entry Plug.dc.html` | Protótipo interativo, v1 — 9 telas |
| `Entry Plug v2.dc.html` | Protótipo interativo, v2 — mesmas 9 telas, palheta revisada, painel SEELE, scanline |
| `support.js`, `_ds/` | Runtime do Claude Design. Não é produto |
| `uploads/*.png` | Frame do anime usado como referência de mood |

**Telas cobertas:** `01 BOOT` · `02 AUTENTICAÇÃO` · `03 PRINCIPAL` · `04 EM CHAMADA` · `05 BATERIA` · `06 ALERTA` · `07 TERMINAL DOGMA` · `08 MOBILE` · `09 INVENTÁRIO`.

Isso cobre os seis estados visuais exigidos por `05`, mais Terminal Dogma, mobile e um inventário de componentes.

### Por que este design é melhor do que o esperado

O protótipo **não é só a casca gráfica**. Ele traz um renderizador de grid de células — funções `zip`, `row`, `rule`, `cel`, `pd`, `pl`, `esmaecer`, `sobrepor` — que desenha as mesmas telas em modo terminal, com box-drawing e 256 cores ANSI. Ou seja: o design já respondeu a pergunta que `06` exige de toda tela gráfica, *"como isso ficaria em 80×24 monocromático?"*, e respondeu em código.

Consequência prática: **os tokens são a fonte para as duas cascas**, não só para o Tauri. `seele-tokens.json` já traz `ansi256` e `ansi16` por cor, que é exatamente o que `05` precisa para a degradação truecolor → 256 → 16.

### Achados que precisam de decisão

**D11 · A palheta dos tokens está desatualizada em relação ao v2.**

| Token | `seele-tokens.*` e v1 | protótipo v2 |
|---|---|---|
| `laranja-nerv` | `#FF6B00` | `#F2521F` |
| `vermelho-alerta` | `#E01B24` | `#FF1A1A` |
| `fosforo` | `#3DF57A` | `#6BFFB6` |

As demais cores (osso, linha, azul, negros) são idênticas. v2 é o artefato mais recente e a revisão parece deliberada — laranja mais avermelhado, vermelho mais puro, fósforo mais claro.

> **Recomendação:** adotar a palheta do v2 e **regenerar** `seele-tokens.json`/`.css` a partir dela, recalculando os índices `ansi256` e `ansi16` — os atuais foram derivados dos valores do v1 e não valem para o v2. Verificar contraste do osso sobre negro-painel no processo.
>
> **Motivo:** duas palhetas concorrentes no repositório garantem que alguém construa contra a errada. Custa duas horas agora e um retrabalho de tela inteira em M4. É a tarefa `M0.12`.

**C9 · O protótipo usa a porta `7743`, a spec usa `8383`.**
A tela de autenticação mostra `seele://toquio-3.dogma.central:7743`. `01` e `04` dizem `8383`. Decisão D10.

**Esquema de URI `seele://` não existe em spec nenhuma.**
O protótipo o usa como forma canônica de endereçar um Dogma. É boa ideia — `:conectar <host>` de `05` fica mais claro com um esquema explícito — mas precisa ser especificado (host, porta, path opcional para o Cage) antes de M2. Adicionar a `02`.

**A scanline do v2 tende a violar `07`.**
v2 adiciona `.seele-scan` (overlay de linhas) e um keyframe `seeleVarredura` que translada continuamente. `07` diz *"sem transição decorativa"* e *"movimento é diagnóstico"*. Um overlay estático de scanline é textura e passa; uma varredura em movimento perpétuo é decoração e não passa pela própria regra do tema. Recomendação: manter a textura, cortar a animação — ou aceitar explicitamente como exceção única e registrar.

**`_ds/_ds_manifest.json` é lixo de outro projeto.**
Lista cards `05-camera-card.html`, `06-tabela-animais.html`, `11-pagina-cameras.html` — metadados de um design system que não é o SEELE. `components`, `tokens` e `themes` estão vazios. Ignorar; não é fonte de verdade. Não apagar sem confirmar que o Claude Design não precisa dele para reabrir o projeto.

**O PNG é um frame do anime.**
`design/uploads/pasted-1786132126320-0.png` é a tela do SEELE da série. Serviu de referência de mood e cumpriu o papel. **Não pode ser distribuído com o produto nem ficar num repositório público.** Ver D9.

### Como o design entra no plano

Nada disso muda M0/M1 além de `M0.12`, porque M1 não tem interface. O que muda é que **M4 e M5 deixam de ter um bloqueio aberto**:

- `07` dizia que os valores definitivos de cor "saem do trabalho no Claude Design" — saíram.
- `09` (M5) exige "implementação do design entregue pelo Claude Design" — está em `design/`.
- A degradação para 256 e 16 cores exigida por `05` tem mapeamento pronto nos tokens.

O que **continua** faltando para M4: os tokens não cobrem o **modo sem cor** exigido por `05` (só forma e texto, respeitando `NO_COLOR`). O protótipo tem modo terminal, mas não modo monocromático. Isso vira tarefa de M4, não de M0.

---

## 7. Contradições e lacunas nas specs

Levantadas na leitura, não resolvidas silenciosamente.

### Contradições

**C1 — Orçamento de latência não fecha.**
`00` fixa < 60 ms boca-a-ouvido em LAN. `03` define alvo inicial de jitter buffer em 40 ms e quadro Opus de 20 ms. Só esses dois somam 60 ms, antes de buffer de captura, encode, rede, decode e buffer de reprodução. `09` transforma os 60 ms em critério de aceite verificável de M1. Como está escrito, **M1 não pode passar**. → D2.

**C2 — Momento da decisão de autenticação.**
`09` (M0) exige *"decidir e registrar: ... mecanismo de autenticação"*. `08` marca a mesma decisão como *"[EM ABERTO — escolher em M2]"*. Datas incompatíveis para a mesma decisão. → D8.

**C3 — Sessão retomável entre clientes vs autenticação por chave pública.**
`06` e `09` (M5) exigem *"mesma sessão retomável entre TUI e app, sem perda de histórico"*. `08` recomenda par de chaves gerado **no primeiro uso do cliente** e reconhece que multi-dispositivo *"precisa de fluxo próprio"* — mas esse fluxo não existe em spec nenhuma. Dois clientes na mesma máquina são duas identidades distintas pelo modelo recomendado. Falta o desenho de vínculo de dispositivos.

**C4 — FEC do Opus.**
`03` lista *"FEC in-band: **Ligado**"* como parâmetro fechado. `02` pergunta em `[EM ABERTO]` se FEC entra em v1. Um dos dois está errado. → D3.

**C5 — Faixa de bitrate.**
`03` especifica 24–48 kbps na tabela e, três linhas abaixo, *"cai para 16 kbps sob perda > 5%"*. 16 está fora da faixa declarada.

**C6 — Erro de sinal na penalidade de RTT.**
`02` escreve `penalidade_rtt(rtt_ms) # 0 acima de 40 ms, cresce até 40 pontos`. Está invertido: a penalidade deve ser 0 **abaixo** de 40 ms e crescer acima. Como está, uma conexão ruim é a que não sofre penalidade.

**C7 — "Não há caminho não criptografado" vs "UDP simples" em M1.**
`01` e `08` são categóricos: TLS 1.3 obrigatório, sem flag para desabilitar. `09` (M1) especifica *"UDP simples"*. É reconciliável — o PoC é descartável e não é o produto — mas duas consequências merecem registro: a latência medida em M1 **não inclui** overhead de QUIC/TLS, então o número de M1 não é o número do produto; e `09` deveria dizer isso.

**C8 — Vermelho de uso exclusivo.**
`07` reserva vermelho para *"erro e queda"*. As faixas da Taxa de Sincronização usam vermelho para `< 40 crítico`, que é degradação sem erro. Menor, mas a palavra "exclusivo" não se sustenta.

**C9 — Porta: `8383` (spec) vs `7743` (design).** → D10, seção 6.

### Lacunas

**G1 — `ssrc` nunca chega ao cliente.**
`02` diz que o cliente resolve `ssrc → usuário` *"pela tabela recebida no controle"*, mas nenhuma mensagem servidor→cliente carrega `ssrc`: `Sessao` traz id/dogma/cages/papéis, `UsuarioEntrou` traz `cage_id` + perfil. Falta também o cliente saber o **próprio** `ssrc`. Buraco de protocolo — fechar antes de M2.

**G2 — Vínculo `ssrc` ↔ conexão não é declarado.**
`08` diz *"`ssrc` atribuído pelo servidor, nunca aceito do cliente"* e `02` diz que o servidor *"encaminha íntegro"*. Junto, isso implica que o servidor deve **verificar** que o `ssrc` no datagrama é o que ele atribuiu àquela conexão, e descartar caso contrário. `04` insinua (*"datagram de um `ssrc` conhecido"*) mas nunca enuncia a regra. Como está, um cliente pode forjar o `ssrc` de outro — exatamente a ameaça que `08` diz tratar.

**G3 — Glossário bilíngue incompleto.**
`10` declara o glossário de `07` normativo **nos dois idiomas** e dá três exemplos (`Cage`→`Cage`, `Piloto`→`Pilot`, `Taxa de Sincronização`→`Sync Ratio`). `07` é inteiramente em português. Não há termo inglês definido para `Dogma Central`, `PADRÃO AZUL`, `PADRÃO LARANJA`, `A.T. Field`, `Isolamento total`, `Bateria interna`, `Distúrbio harmônico`, `Terminal Dogma`, `Inserir plug`, `Ejetar`. Como `10` manda escrever **código em inglês**, esses são nomes de tipo e de variante de erro. → `M0.7`.

**G4 — i18n não tem milestone.**
`10` exige i18n desde o início, com pt-BR e en, e proíbe literal de string na interface. Nenhum dos sete milestones de `09` menciona i18n. Retrofit de i18n é exatamente o custo que `10` diz querer evitar. → `M0.9`.

**G5 — Silêncio DTX/VAD indistinguível de perda.**
`03` liga DTX e usa VAD; `02` diz que `ts` *"detecta gaps de silêncio"*, mas nenhum documento diz **como** a métrica de perda exclui esses gaps. Consequência em `07`/`02`: a Taxa de Sincronização despenca em silêncio. → R5, `M1.9`.

**G6 — Deriva de clock não é mencionada.**
Nenhuma spec trata skew entre relógios de dispositivos. → R3, `M1.8`.

**G7 — Reamostragem só existe na captura.**
O pipeline de captura em `03` tem *"conversão para 48 kHz mono f32"*. O de reprodução vai do mixer direto ao ring buffer e ao `cpal`, sem conversão. Dispositivo de saída a 44,1 kHz — padrão de fábrica em muito hardware, inclusive macOS — não é tratado. → `M1.4`.

**G8 — "Isolamento total" (surdo) não existe no protocolo.**
`07` define o termo, `05` dá a tecla `d`. `02` tem `DefinirATField` para mudo, mas nada para surdo, e `EstadoUsuario` lista apenas A.T. Field, presença e sync. Ser surdo é local, então tecnicamente não exige protocolo — mas o roster dos outros pilotos não tem como mostrar quem não está ouvindo.

**G9 — `client_msg_id` fora do payload.**
Em `02`, `EnviarMensagem` tem payload `linha_id, corpo, responde_a opcional`, e a coluna de notas diz *"idempotente por `client_msg_id`"*. O campo não está no payload.

**G10 — 0-RTT sem tratamento de replay.**
`01` promete 0-RTT em reconexão. `08` não menciona que dados de 0-RTT são replicáveis por um atacante e que, portanto, nenhum comando que altere estado pode viajar em 0-RTT. Lacuna de segurança dentro do documento de segurança.

**G11 — `PROMPT-CLAUDE-DESIGN.md` não existe.** → **RESOLVIDA.** O design chegou como `design/`. Ver seção 6. Resta atualizar as referências em `06` e `09` para apontar para `design/`, já que o arquivo citado nunca existiu.

**G12 — Detecção de queda leva 15 s.**
`02` define `Ping` a cada 5 s com três perdidos consecutivos para entrar em `Reconectando`, enquanto `00` promete recuperação transparente. São 15 segundos de áudio morto antes de a interface reconhecer qualquer coisa. O plano de mídia detecta ausência de datagrams em ~200 ms; a spec não conecta as duas coisas.

**G13 — `ssrc` é redundante no sentido cliente→servidor.**
Uma conexão QUIC é um cliente é um `ssrc`; o servidor já sabe quem enviou. São 4 dos 11 bytes de cabeçalho gastos por nada nesse sentido (necessários no sentido servidor→cliente). `02` diz *"não adicionar campos sem necessidade demonstrada"* — vale a simetria inversa. Não é erro; é observação para quando o cabeçalho for congelado em M2.

**G14 — Modo sem cor não tem tokens.**
`05` exige modo sem cor (só forma e texto) e respeito a `NO_COLOR`. O design entregue cobre truecolor e terminal em 256 cores, mas não define o modo monocromático. Tarefa de M4. → seção 6.

---

## Apêndice — resumo de esforço

| Milestone | Tarefas | Pontos | Sobrevive | Descartável | Ferramenta |
|---|---|---|---|---|---|
| M0 | 12 | 26 | 10 | 0 | 1 |
| M1 | 17 | 61 | 10 | 2 | 3 |

A razão M1 : M0 ≈ 2,3 é a expressão numérica do que `09` diz em texto: áudio é o milestone caro, e ele vem primeiro por isso.
