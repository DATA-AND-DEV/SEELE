# Relatório do lote P54 (F5, 0.16)

«PADRÃO DA MÁQUINA · <aparelho>» só diz o aparelho quando o padrão é a escolha. Pendência 54.

- Worktree: `/Users/dev-alexandre/SEELE/.claude/worktrees/agent-a76bd5477d45536ce`
- Branch: `0.16/lotes-p54`, aberto em `fbeff8e`.
- Status: DONE_WITH_CONCERNS. As preocupações estão no fim e nenhuma bloqueia.

## O que foi feito

**Commit que fecha a pendência 54: `0d68c95`.**

Em `marcarUmaLista` (`apps/seele-app/ui/tela-server.js`), a linha do padrão escrevia o nome do aparelho aberto sem olhar a escolha: `const resolvido = aberto === null ? null : nomeDe[chave].get(aberto);`. Agora ela decide pelo `escolhidoDe[chave]`:

| Escolha (`escolhidoDe[chave]`) | O que a linha diz |
|---|---|
| `null` (o padrão é a escolha) e há aparelho aberto | `PADRÃO DA MÁQUINA · <aparelho que a voz abriu>`, como hoje |
| `null` e não há sessão | `PADRÃO DA MÁQUINA`, como hoje |
| um aparelho escolhido à mão | `PADRÃO DA MÁQUINA · <o aparelho da linha com data-padrao="sim">`, que é o padrão do sistema |
| um aparelho escolhido à mão e a máquina não marcou padrão nenhum | `PADRÃO DA MÁQUINA`, sem nome |

O brief deixava duas saídas para o caso do aparelho escolhido à mão: dizer o padrão do sistema, ou nada. Escolhi dizer o padrão do sistema, porque é a informação que a linha tem a oferecer: é o que a pessoa teria se a escolhesse de volta. Com o monitor 27QHD240 escolhido e os alto-falantes do MacBook como padrão, a tela passa a mostrar `PADRÃO DA MÁQUINA · Alto-falantes (MacBook Pro)` e, abaixo, `27QHD240 EM USO`. Antes mostrava `PADRÃO DA MÁQUINA · 27QHD240`.

O nome do padrão do sistema vem do próprio DOM da lista (`lista.querySelector('.server-dispositivo[data-padrao="sim"] .server-dispositivo-nome')`). Aquela linha nunca é reescrita por `marcarUmaLista`, que só renomeia a linha com `id === null`, então ler o texto dela é seguro e não cria uma terceira cópia dos nomes. O texto da linha é escrito em todos os ramos, então não sobra nome velho de uma leitura para a seguinte.

Não mudei o que o Snapshot traz, nem `frontend.rs`, nem o fio. O comentário de `nomeDe` ganhou «quando o padrão é a escolha».

## TDD

O caso novo está em `apps/seele-app/bancada/qualidade-da-voz.cjs` (`P54 OK`), entre o caso da régua de sensibilidade e o do medidor. Ele é Playwright sobre a janela de verdade (`#tela-server`, `desenharDispositivos()` e `atualizarServer()` reais, ponte simulada), e roda os dois lados, microfone e saída, com nomes diferentes de propósito. São cinco cenas:

1. o padrão é a escolha e a voz abriu o padrão do sistema;
2. o padrão é a escolha e a voz abriu outro aparelho (a linha segue dizendo no que o padrão deu);
3. outro aparelho escolhido à mão e aberto (o defeito, com `microfone_escolhido`/`saida_escolhida` igual ao outro e `snapshot.capture.id`/`playback.id` igual ao escolhido). Também confere as marcas: `EM USO` no escolhido, `PADRÃO` no padrão do sistema, nada na linha do padrão;
4. outro aparelho escolhido à mão, sem sessão;
5. outro aparelho escolhido à mão e nenhum aparelho com `default: true`.

### RED (antes do conserto)

```
PLAYWRIGHT=/Users/dev-alexandre/SEELE-MOD-PERFIS/node_modules/playwright perl -e 'alarm 240; exec @ARGV' node apps/seele-app/bancada/qualidade-da-voz.cjs
```

```
F02 OK: o marcador e o padrão da sensibilidade seguem o filtro
F01 OK: a régua vem do Rust e distingue escolha de padrão
AssertionError [ERR_ASSERTION]: microfone, outro aparelho escolhido à mão, e é ele que está aberto: com «Interface USB» escolhido à mão a linha do padrão traz o nome dele, e quem a lê entende que o padrão da máquina é «Interface USB» — o padrão do sistema é «Microfone (MacBook Pro)»: PADRÃO DA MÁQUINA · Interface USB
+ actual - expected
+ 'PADRÃO DA MÁQUINA · Interface USB'
- 'PADRÃO DA MÁQUINA · Microfone (MacBook Pro)'
    at .../apps/seele-app/bancada/qualidade-da-voz.cjs:311:14
```

Vermelho pelo motivo certo: as cenas 1 e 2 (comportamento que não pode mudar) passaram no código velho, e falhou a cena 3, que é o defeito. A bancada para na primeira asserção que falha, e o microfone vem antes da saída no laço.

### GREEN (depois do conserto)

```
F02 OK: o marcador e o padrão da sensibilidade seguem o filtro
F01 OK: a régua vem do Rust e distingue escolha de padrão
P54 OK: a linha do padrão só diz o aparelho quando o padrão é a escolha
F01 OK: o medidor mostra o nível, o corte e quantas vezes abriu
F01 OK: fechar a tela fecha o microfone
```

## A prova por reversão

Com o conserto no arquivo (sha1 `94cf6148…`), voltei só a linha de hoje no lugar do bloco novo (`const resolvido = aberto === null ? null : nomeDe[chave].get(aberto);`) e rodei a bancada. Ficou vermelha com a frase do caso, a mesma do RED acima:

```
AssertionError [ERR_ASSERTION]: microfone, outro aparelho escolhido à mão, e é ele que está aberto: com «Interface USB» escolhido à mão a linha do padrão traz o nome dele, e quem a lê entende que o padrão da máquina é «Interface USB» — o padrão do sistema é «Microfone (MacBook Pro)»: PADRÃO DA MÁQUINA · Interface USB
```

Depois restaurei a cópia guardada do arquivo consertado: o `shasum` voltou a `94cf61485ee9d2e9e162b1ffe3d243a4e599146e` e o `cmp` deu igual, byte a byte. A bancada voltou ao verde.

## Os testes que rodaram

- `node apps/seele-app/bancada/qualidade-da-voz.cjs` (Playwright do `SEELE-MOD-PERFIS`, nada baixado): verde, 5 casos.
- `cargo test -p seele-app --test frontend the_screen_says_which_microphone`: 1 passou. O guarda de texto que exige `EM USO` e `ESCOLHIDO` no corpo de `marcarUmaLista` segue verde.
- `cargo xtask check-runtime`: saiu 0 (as 7 bancadas dele). Ele **não** inclui `qualidade-da-voz.cjs`; essa roda no `ci.yml:296`. Por isso o check-runtime verde não diz nada sobre o caso novo, e a prova dele é a execução direta acima.
- `cargo fmt --all -- --check`: ok. Não toquei Rust, então não rodei clippy.

## Arquivos mudados

- `apps/seele-app/ui/tela-server.js`
- `apps/seele-app/bancada/qualidade-da-voz.cjs`
- este relatório

Não toquei `apps/seele-app/tests/frontend.rs`, `docs/pendencias.md` nem o índice da 1.0. A pendência 54 fica registrada como fechada pelo `0d68c95` no lote Z.

## Para a v6

Nada. Nenhuma promessa pública nova: evento, campo ou frase de `api/README.md`.

## O que não rodou aqui

- **O WKWebView (macOS) e o WebView2 (Windows) de verdade.** A bancada é o Chromium do Playwright sobre o `ui/` servido como está, com a mesma CSP do produto e a ponte do Tauri simulada. O conserto usa só `querySelector` com seletor de atributo, `?.` e `textContent`, que o arquivo já usa e que os dois motores têm. A confirmação nas janelas nativas é do dono, como diz o brief.
- Nada de áudio, nem aparelho de verdade: os aparelhos são os da tabela da bancada. Não se mediu o que `default` traz do `cpal` em cada sistema, só que a tela o respeita.

## Preocupações

1. **A escolha entre «padrão do sistema» e «nada».** O brief aceitava as duas; escolhi o padrão do sistema. Se o dono preferir a linha sem nome quando há aparelho escolhido, é trocar o ramo `escolhidoDe[chave] !== null` por `resolvido = null` e a cena 3 passa a esperar `PADRÃO DA MÁQUINA` puro.
2. **Aparelho escolhido que sumiu.** Se o aparelho escolhido não está mais na lista (a interface desconectada), `escolhidoDe[chave]` continua não nulo e a linha do padrão diz o padrão do sistema. Está certo, porque o padrão não é a escolha, mas a bancada não tem cena para isso.
3. **A reversão não mostra o lado da saída vermelho por conta própria.** O laço da bancada para na primeira asserção que falha, o microfone. Os dois lados passam pela mesma função, então o conserto e a reversão valem para os dois, e a cena verde confere os dois, mas só o lado do microfone foi visto vermelho.
4. **O padrão do sistema duplicado na tela.** Com um aparelho escolhido, o nome do padrão aparece duas vezes: na linha do padrão e na linha do próprio aparelho (marcada `PADRÃO`). Não contradiz nada e é o que a linha existe para dizer, mas é uma repetição que o dono pode achar ruidosa ao ver na janela nativa.
