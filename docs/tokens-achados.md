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

`specs/06-clientes-gui.md` exige modo alto contraste e nota que a paleta depende
muito de vermelho e verde. **Trabalho de M4:** ou subir `osso-apagado` para
≥ 4,5:1, ou aceitar e garantir que nenhuma informação necessária use só ele.

**Adendo M5:** a varredura passou a cobrir a janela inteira, e um véu cobra
deste número. Ver "O que a varredura custa de contraste", no fim deste arquivo.

**Resolvido.** A primeira das duas opções: `--seele-rotulo-painel` deixou de ser
apelido de `osso-apagado` e passou a ser cor própria, `#908574`, que mede
**5,52:1** sobre `negro-painel`. A mira acima de 4,5:1 é deliberada e o motivo
está no adendo M5 desta página: sob a varredura a 6% o mesmo par entrega 4,97:1,
e dentro da faixa que desce, 4,81:1. Uma cor escolhida para bater 4,5:1 no vácuo
chegaria à tela abaixo de AA nas duas condições em que a tela é de fato vista.

`osso-apagado` **não** subiu, e continua valendo o que valia: ele segue servindo
o que é texto grande e o que não é informação necessária. O que mudou de dono foi
o rótulo miúdo.

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

`specs/06-clientes-gui.md` exige modo sem cor (só forma e texto) e respeito a
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

> **Correção (M5).** A frase acima está errada em dois tempos, e o segundo é
> culpa deste documento. Nada de textura foi adotado em M0.12: até `4a7ca88`,
> `apps/seele-app/ui/seele.css` não tinha scanline nenhuma — nem estática, nem
> animada. "Adotei a textura estática" descrevia uma intenção como se fosse
> código. E desde M5 a condição do parágrafo foi cumprida: a varredura existe,
> inteira, textura e animação, como exceção nomeada — ADR 0014, revisão em M5.

## O que a varredura custa de contraste (M5)

A varredura é `position: fixed; inset: 0; z-index: 9`: a primeira coisa nesta
interface pintada **sobre** o texto. Nas linhas escuras da textura o véu escurece
o glifo e a superfície juntos, e contraste é razão — escurecer os dois não
preserva a razão, derruba. Por isso não existe véu de graça: qualquer opacidade
maior que zero custa, e o que se escolhe é quanto.

Números por WCAG 2.1 (luminância relativa sRGB, composição em sRGB como o
navegador faz), sobre `negro-painel` — a mais clara das duas superfícies, e
portanto o pior caso para texto claro:

| token | sem véu | véu a 34% (o do comp) | véu a 6% (o que está no ar) |
|---|---|---|---|
| `osso-apagado` | 4,11:1 | **2,35:1** | 3,74:1 |
| `vermelho-alerta` | 5,16:1 | **2,70:1** | 4,63:1 |
| `laranja-nerv` | 5,71:1 | **2,97:1** | 5,13:1 |

A 34%, que é o valor do comp, o estrago não era o achado 3 ficando pior: era o
`osso-apagado` perdendo até o piso de 3:1 que ainda o mantinha válido como texto
grande, e o `vermelho-alerta` caindo a 2,70:1 — a cor cuja aprovação em AA o
achado 2 registra como o argumento mais forte a favor da paleta v2, desfeita em
silêncio por uma textura decorativa.

Está no ar a 6%, e há folga: nem esse é o teto. `vermelho-alerta` (4,5456:1) e
`osso-apagado` (3,6813:1) ainda passam a 7%; 8% inteiro é a primeira reprovação
(`vermelho-alerta` cai a 4,4630:1). O teto de verdade é α ≈ 7,55%, onde
`vermelho-alerta` cruza 4,5:1 — é a cor que decide, porque é a que menos folga
tem. 6% foi escolhido abaixo desse teto de propósito, com margem, não por ser o
maior valor redondo que ainda passava. **O custo que resta** é o da coluna da
direita: `osso-apagado` de
4,11:1 para 3,74:1, `vermelho-alerta` de 5,16:1 para 4,63:1, `laranja-nerv` de
5,71:1 para 5,13:1. Ninguém muda de classificação, mas a folga do vermelho
encolheu — o que M4 fizer com `osso-apagado` tem agora um segundo número para
respeitar.

Quase nada se perdeu de aparência ao descer de 34% para 6%, e a razão é
desconfortável: o véu é `negro-absoluto` sobre um fundo de `negro-absoluto`.
Em `html, body` a textura é **exatamente** um no-op em qualquer opacidade. A
única superfície onde ela chegava a aparecer é o painel, e mesmo a 34% ele ia de
(10 · 8 · 6) a (8,3 · 6,6 · 5,0) — menos de dois níveis de 255. A textura nunca
foi visível onde deveria estar. Ela era visível no texto.

`apps/seele-app/tests/tokens.rs` mede isto para o véu estático (`.varredura`,
6%): lê a opacidade que a folha declara, refaz a conta e reprova se algum dos
três cair abaixo do critério que já cumpria. Subir o véu de novo é possível, mas
passa a ser uma decisão, não uma edição.

O teste **não** mede a faixa que desce (`.varredura::after`). Dentro da faixa de
5vh, a opacidade efetiva é 0,68 × 0,06 ≈ 4% de `osso-apagado` — uma cor clara —
somada por cima do véu já aplicado, e por isso *clareia* em vez de escurecer.
Sobre o painel velado, isso deixa `vermelho-alerta` em ≈4,39:1, abaixo do piso
de 4,5:1, e `osso-apagado` em ≈3,66:1. Não é regressão: a faixa sempre teve
~4% de `osso-apagado` (0,12 × 0,34 antes, 0,68 × 0,06 agora), e a mudança de M5
só melhorou o número, nunca piorou. A faixa também é transitória e ocupa 5vh da
janela. Por isso a faixa não muda — mas o número fica registrado aqui em vez de
não examinado, porque a frase acima só cobre o que o teste de fato cobre.
