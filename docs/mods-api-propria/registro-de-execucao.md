# Registro de execução — API própria de MODs

O registro que o [roteiro](roteiro-de-execucao.md) §8 pede. Cada entrada diz o
que mudou, onde, que comportamento foi **provado**, o que falta e qual etapa
pode começar.

| Etapa | Estado | Evidência |
| --- | --- | --- |
| E0 — inventário | **Feito** | §1 e §2 abaixo |
| E1 — executor e autoridade | **Medido; decisão pendente** | §3 abaixo, `apps/seele-app/testes/sonda-de-fronteira/` |
| E2 — sessão e encerramento | Pendente | — |
| E3 — renderer e SDK mínimos | Pendente | — |
| E4 — desenho e mídia | Pendente | — |
| E5 — funções completas | Pendente | — |
| E6 — publicação preparada | Pendente | — |

---

## 1. E0 — onde cada repositório estava

Conferido em 18/09/2026, depois de `23fc654`.

| Repositório | HEAD | Trabalho local não commitado |
| --- | --- | --- |
| SEELE | `23fc654` | os documentos desta proposta, ainda não commitados |
| SEELE-MODS-INDEXER | `251c7a0` | guia, `ferramentas/`, `manifesto.py`, testes e site — **trabalho de outra pessoa em curso** |
| SEELE-MOD-MESA | `7ca40d1` | migração para API 3 |
| SEELE-MOD-PERFIS | `ce976fd` | migração para API 3 |
| SEELE-MOD-ESTILO | `0fba9a5` | migração para API 3 |
| SEELE-RELEASES | `4750acc` | — |
| SEELE-SITE | `39a4b73` | — |

**Nada foi descartado.** Os três MODs e o indexador têm alterações de outra
pessoa na árvore; este trabalho não as tocou e não as commitou.

## 2. E0 — a matriz funcional, medida

Não é opinião sobre a API: é a contagem do que cada cliente chamava antes e
chama agora. «Antes» é o `cliente/main.js` commitado (API 2); «agora» é o da
árvore de trabalho (API 3).

| MOD | Linhas antes → agora | O que o cliente de API 2 usava | O que o de API 3 usa |
| --- | --- | --- | --- |
| MESA | 413 → 90 | `canvas`, `dialog`, `button`, `section`, `h*`, dois `FormData`, `FileReader`, `Image`, cliques | quatro formas de leitura |
| PERFIS | 294 → 74 | `FileReader`, `Image`, `change`, `cancel` | quatro formas de leitura |
| ESTILO | 146 → 72 | `cancel`, tokens no documento | quatro formas de leitura, `SeeleUI.tema` |

**Os três clientes de hoje são telas de leitura.** A afirmação da proposta —
«carregou os pacotes e não manteve os produtos» — confere com o código. Nenhum
deles tem entrada, mídia, desenho ou arquivo.

## 3. E1 — a fronteira real do executor, medida

### Como foi medido

`apps/seele-app/testes/sonda-de-fronteira/` é um MOD que **tenta** cada caminho
e grava o resultado no quintal da metade de servidor. Ele rodou no aplicativo
nativo — WKWebView, macOS —, num `SEELE_HOME` descartável, com o servidor
hospedado pelo próprio app. A casa de verdade de quem trabalha aqui não foi
tocada.

O §3.1 do contrato é a razão de ele existir: «não ter `document`, `window` ou o
objeto global do Tauri não prova isolamento de armazenamento, rede, IPC ou
canais entre contextos», e «o código de teste deve tentar acessar diretamente os
caminhos alternativos».

### O que está fechado

| Caminho | Resultado |
| --- | --- |
| `document`, `window`, `__TAURI__`, `__TAURI_INTERNALS__` | não existem |
| `localStorage` | não existe (workers não o têm) |
| `SharedWorker` | não existe |
| `fetch` para origem externa | recusado pela CSP (`TypeError`) |
| `Worker` filho depois do `terminate()` do pai | **morreu junto** |

A última linha merece o detalhe, porque o ADR 0049 promete que «`terminate()` é
garantia e não pedido». O filho gravava a hora a cada meio segundo. O MOD foi
desligado às `1789785276`; a última batida do filho foi `1789785278,373` — 2,4
segundos depois, que é a latência de anúncio, desconexão e desmontagem. A
medição seguinte, dezoito segundos mais tarde, leu **a mesma** última batida. O
filho não sobreviveu ao pai.

### O que está aberto

| Caminho | Resultado | O que isso significa |
| --- | --- | --- |
| `indexedDB` | **alcançou, e o que ele gravou sobreviveu** | ver abaixo |
| `caches` | alcançou, abriu | mesmo armazenamento de origem |
| `BroadcastChannel` | alcançou, abriu e postou | dois MODs conversam por fora da API |
| `fetch ipc://localhost/<cmd>` | **alcançou** — HTTP 500 | ver abaixo |

**O armazenamento sobrevive à saída.** O MOD gravou uma marca, o aplicativo foi
encerrado, reaberto, e a entrada seguinte no mesmo servidor **achou a marca de
antes**. A promessa «o que um MOD faz some quando você sai» é **falsa hoje para
armazenamento**, e nenhum `terminate()` conserta isso: ele mata o contexto e não
toca no armazenamento da origem. Um worker de `blob:` herda a origem de quem o
criou, e a origem é a do produto.

**O IPC do Tauri é alcançável.** O caminho existe e a CSP o permite — este
`connect-src` tem `ipc:` porque é assim que a própria janela invoca. O que
recusou foi o `Tauri-Invoke-Key`: o Tauri 2.11 sorteia dezesseis bytes por
execução, injeta-os como constante no escopo do script da janela e os confere no
Rust (`webview/mod.rs:1748`, HTTP 500 quando não batem). Um worker não alcança
aquele escopo, então hoje ele não passa.

Mas a fronteira é **o segredo**, e não a ausência de `document`. Isso é
exatamente o que o §3.1 do contrato manda não assumir, e a diferença importa: o
segredo vive na mesma janela que monta a fonte do worker, e qualquer código que
um dia interpolasse um valor ali o entregaria.

### O que isto decide, e o que não decide

**Não decide o executor.** O Worker passou nos testes de ambiente de janela e
reprovou nos de armazenamento e de canal entre contextos. As duas saídas do
contrato continuam abertas:

- **manter o Worker e fechar os três buracos.** `indexedDB`, `caches` e
  `BroadcastChannel` não se apagam de dentro do worker por `delete` — o §3.1
  proíbe aceitar isso como barreira, e está certo: o código do MOD roda depois
  do prelúdio e pode guardar as referências antes. Fechar de verdade exige
  origem própria para o executor, o que um `blob:` não dá;
- **trocar por QuickJS no cliente**, que é a alternativa que o contrato nomeia:
  sem ambiente de navegador, não há armazenamento de origem nem canal entre
  contextos para fechar. O custo é que o renderer e a API visual passam a ser a
  **única** superfície, e é preciso medir o que um interpretador em JS custa no
  laço de interface.

**Pendente de medida:** Windows (WebView2) e Linux (WebKitGTK). A sonda está
escrita e o roteiro para rodá-la é o desta seção; o resultado pode diferir,
porque os três motores não compartilham implementação de armazenamento.

### Como repetir

```sh
# publica a sonda num SEELE_HOME descartável, hospeda, e lê o que ela mediu
SEELE_HOME=<descartável> ./target/debug/seele-app --hospedar
sqlite3 <descartável>/seele.db "SELECT key, CAST(value AS TEXT) FROM mod_data;"
```

O pacote precisa estar em `<home>/mods/mod-packages/<hash>` — onde o **servidor**
o procura — e a linha de `mods` com `enabled=1`. O `aceites` precisa da
identidade do conjunto, que o `seele.log` escreve quando a entrada é recusada.

## 4. O que muda no que já está escrito

O ADR 0049 afirma, em «Três consequências que decorrem disso»: «Destruir o
contexto destrói o MOD. Temporizadores, ouvintes, áudio e promessas morrem com
ele». A medição sustenta isso para **execução** e para **descendentes**, e não
para **armazenamento**. A frase precisa da ressalva antes de virar promessa
publicada, e o guia de migração precisa dizer o mesmo.
