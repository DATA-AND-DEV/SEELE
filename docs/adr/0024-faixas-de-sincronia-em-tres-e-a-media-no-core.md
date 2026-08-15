# 0024 — Faixas de sincronia em três, e a média do Cage no core

Status: aceito

## Contexto

`specs/07-tema-evangelion.md` bandava a Taxa de Sincronização em quatro: `≥ 90` nominal, `70–89` aceitável em osso, `40–69` degradado, `< 40` crítico. O código obedecia, em `seele_proto::sync_ratio::SyncBand`, e as duas cascas pintavam a partir daí.

O comp `design/Entry Plug v2.dc.html` banda o mesmo número em três:

```js
function corSync(v){ if (v >= 85) return FOS; if (v >= 60) return LAR; return VER; }
```

`≥ 85` fósforo, `60–85` laranja, `< 60` vermelho — e osso não aparece em escala de sincronia nenhuma. O comp é posterior à tabela de `07`, e o dono decidiu que ele vence.

O mesmo comp mostra uma **MÉDIA DO CAGE**: a média das taxas dos pilotos de um Cage, colorida pela faixa. No comp, quem calcula e quem colore é a casca.

## Decisão

**Três faixas.** `SyncBand` fica com `Nominal`, `Degraded` e `Critical`, cortando em 85 e 60. `Acceptable` some. `specs/02` e `specs/07` foram corrigidos para dizer o mesmo — a spec passa a descrever o que o produto faz.

**A média mora no core.** `seele_core::Room::cage_sync(cage) -> Option<CageSync>` devolve a média já bandada, e as duas cascas a leem: a TUI direto do `Room`, o app pelo `Cage.sync` do `Snapshot`. É a mesma regra que decidiu a busca e que `crates/seele-ffi/src/types.rs` já defendia para a faixa de cada piloto: *"duas cascas com duas cópias de 'X é nominal' são duas cascas que discordam no dia em que uma delas for atualizada."*

**Cage vazio não tem média.** `None`, não zero. Zero é uma medição, e pelas faixas é uma medição crítica: um Dogma com quatro Cages ociosos mostraria quatro salas vermelhas onde não há ninguém, e o Cage que está de fato em apuros deixaria de saltar aos olhos.

**Quem ejetou não conta.** Ejetar sai dos assentos e mantém o nome — `seats` perde o piloto, `pilots` guarda. A média é sobre os assentos, então quem saiu para de pesar no instante em que sai, e não puxa a sala para baixo de fora dela. Não existe terceiro estado: este cliente não tem noção de piloto que está no Cage e ejetado.

**Sem decimal.** O comp imprime `98.4`; o dado é `u8` em todo ponto onde existe — no fio, em `PilotState.sync_ratio`, na suavização. A média é arredondada ao ponto inteiro mais próximo, empate para cima, em aritmética inteira: `(2·soma + n) / 2n`. Um decimal na tela seria precisão inventada no último passo. O protocolo não muda por causa disto.

## Consequências

Remover uma variante de `SyncBand` é **quebra** de duas fronteiras. Vale a pena escrever por onde ela passa e por onde não passa:

- **Não atravessa o fio.** Nenhuma mensagem de `seele-proto` carrega a faixa; o que trafega é o `u8`. O `postcard` codifica variante por índice, de modo que uma remoção *seria* silenciosa — e não é problema aqui porque nada a serializa para o fio nem para disco.
- **Atravessa a FFI pelo nome.** O `Snapshot` vira JSON para a webview, e a faixa vai como `"Nominal"`, `"Degraded"`, `"Critical"`. Um `data-faixa="Acceptable"` do lado do JS deixa de casar com qualquer coisa.
- **O `Deserialize` derivado passa a recusar `"Acceptable"`.** É o comportamento desejado: um valor guardado por uma versão antiga estoura em vez de cair calado na faixa vizinha. Há teste que cobra isso.

O que muda na tela, e é o objetivo: 80 lia-se como fora do nominal e tudo bem; agora se lê como degradado — laranja, a cor de ir olhar.

Sobra dívida em `apps/seele-app/ui/`, deliberadamente não tocada aqui porque outra tarefa é dona daqueles arquivos: a entrada `Acceptable` do mapa `marcaSync`, a regra `.sync[data-faixa="Acceptable"]` e o token `--seele-sync-aceitavel` ficam sem uso, e `Cage.sync` chega ao `Snapshot` sem ninguém desenhar. `design/seele-tokens.{css,json}` ainda descrevem a escala de quatro; ADR 0014 diz qual arquivo de tokens é canônico, e mexer nele é trabalho próprio, não de passagem.
