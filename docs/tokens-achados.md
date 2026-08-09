# Tokens de design — achados ao congelar (M0.12)

Ao reconciliar `design/seele-tokens.*` com o protótipo v2 e recalcular os índices
ANSI e o contraste, apareceram seis coisas. Registradas aqui porque quatro delas
viram trabalho em M4.

Método: vizinho mais próximo em CIELAB restrito aos índices **16–255**. Os
índices 0–15 são deliberadamente excluídos — terminais deixam o usuário
retematizar essa faixa, então casar um token com eles produz cor imprevisível.
Contraste por WCAG 2.1 sobre `negro-painel`.

## 1. O arquivo de tokens entregue tinha um índice ANSI errado

Independente da questão v1 × v2: `seele-tokens.json` dizia `ansi256: 208` para o
laranja `#FF6B00`. O índice 208 é `#FF8700`; o vizinho correto é **202**
(`#FF5F00`). O verde 84 e o vermelho 160 estavam certos para os valores do v1,
mas mudam com o v2.

| token | v1 hex | v1 ansi declarado | v2 hex | v2 ansi calculado |
|---|---|---|---|---|
| `laranja-nerv` | `#FF6B00` | 208 ❌ (era 202) | `#F2521F` | **202** |
| `vermelho-alerta` | `#E01B24` | 160 ✔ | `#FF1A1A` | **196** |
| `fosforo` | `#3DF57A` | 84 ✔ | `#6BFFB6` | **85** |

## 2. A revisão v2 consertou uma reprovação de acessibilidade

Esse é o argumento mais forte a favor do v2, e não estava explícito em lugar
nenhum:

| token | v1 | v2 | efeito |
|---|---|---|---|
| `vermelho-alerta` | 4,14:1 — **reprova AA** | 5,16:1 — passa AA | corrigido |
| `fosforo` | 13,84:1 | 15,81:1 | melhora |
| `laranja-nerv` | 7,00:1 (AAA) | 5,71:1 (AA) | piora, ainda passa |

O vermelho é a cor que `specs/07` reserva para erro e queda — ou seja, a cor que
mais precisa ser legível estava reprovando em v1.

## 3. `osso-apagado` não passa AA para texto normal

4,11:1. Passa só como texto grande (≥ 3:1). Mas ele é usado justamente para
"rótulo secundário, log inativo", e a escala tipográfica coloca rótulo em **10px
com tracking de 0,22em** — que é texto pequeno.

`specs/05-cliente-tui.md` exige modo alto contraste e nota que a paleta depende
muito de vermelho e verde. **Trabalho de M4:** ou subir `osso-apagado` para
≥ 4,5:1, ou aceitar e garantir que nenhuma informação necessária use só ele.

## 4. Em 256 cores, o painel some dentro do fundo

`negro-absoluto` (`#050403`) e `negro-painel` (`#0A0806`) mapeiam **ambos para
ansi 232**. A diferença entre fundo do app e superfície de painel existe em
truecolor e desaparece em 256 cores.

Não é defeito de cálculo — os dois estão a menos de 1,2 de deltaE do mesmo
cinza. É consequência de a regra de `specs/07` ser "superfície = linha 1px +
vazio": a superfície nunca dependeu do preenchimento. **Confirma a regra**, mas
a TUI precisa saber disso: em 256 cores, o painel é definido pela borda, não
pelo fundo. `specs/05` exige funcionar por SSH em 16 cores sem perder
informação — com mais razão ainda.

## 5. As linhas são invisíveis pelo critério WCAG de componente não textual

`linha` fica em 1,25:1 e `linha-forte` em 1,63:1 contra o fundo. WCAG pede 3:1
para fronteira de componente de interface.

Isso é escolha estética deliberada e não vou mexer nela — a densidade da NERV
depende de linhas discretas. Mas o **modo alto contraste** que `specs/05` exige
não pode simplesmente reusar estes valores; precisa de uma segunda escala de
linha. **Trabalho de M4.**

## 6. O que ainda não existe: modo sem cor

`specs/05-cliente-tui.md` exige modo sem cor (só forma e texto) e respeito a
`NO_COLOR`. O design entregue cobre truecolor e terminal em 256 cores; **não
define o modo monocromático**. Continua sendo a lacuna G14 do plano.

Isso importa mais do que parece, porque `specs/05` também diz que nenhuma
informação pode ser transmitida só por cor. As faixas da Taxa de Sincronização
são quatro cores; sem cor, precisam de quatro formas ou rótulos. O número
sozinho resolve a Taxa, mas A.T. Field, estado de subsistema e severidade de
alerta ainda precisam de marcador textual definido. **Trabalho de M4.**

## Não adotado: a varredura animada do v2

O protótipo v2 acrescentou `.seele-scan` (textura de scanline) e o keyframe
`seeleVarredura`, que translada continuamente. `specs/07` diz "sem transição
decorativa" e "movimento é diagnóstico".

Adotei a textura estática e **deixei a animação de fora**. Se você quiser a
varredura, ela vira exceção única e explícita ao tema, não um detalhe herdado
por acidente.
