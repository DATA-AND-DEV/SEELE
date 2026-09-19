# A fatia interativa no aplicativo nativo — 19/09/2026

Binário: `target/release/seele-app` compilado deste checkout (perfil `release`).
`SEELE_HOME` de descarte, servidor hospedado pelo próprio aplicativo com
`--hospedar`, executor nativo ligado por `SEELE_EXECUTOR=quickjs`.

MOD: `seele/referencia`, publicado pelo conteúdo no `SEELE_HOME` e habilitado
com `enabled=1` na tabela `mods` do servidor. O hash foi conferido contra
`seele_ffi::mods::ler_pasta` antes de ser usado — não foi adivinhado.

## O que foi exercitado, e o que ficou de fora

A execução às 08:49:50 subiu a fatia inteira. Do registro:

```
MOD ativado no executor nativo instancia=1 geracao=1
fala de MOD nativo tipo="mensagem"   (tema)
fala de MOD nativo tipo="mensagem"   (snapshot)
fala de MOD nativo tipo="mensagem"   (pedido ao servidor)
fala de MOD nativo tipo="mensagem"   (região)
mídia de MOD servida caminho=som/toque.wav papel="som" bytes=1644
fala de MOD nativo tipo="mensagem"   (o MOD reagindo a um evento de mídia)
```

Isso prova, no WKWebView de verdade e com o executor nativo:

- **a região monta inteira.** A mídia é a **última** forma declarada; para o
  produto pedir o arquivo dela, as quatro formas antes — campo, botão, tela,
  mídia — passaram por `planejar`, `reconciliar` e `criar`;
- **o caminho da mídia fecha ponta a ponta**: o manifesto declara, `serve`
  entrega, os bytes são reconhecidos como WAV **pelos bytes** e o papel `som`
  volta do Rust. A janela não compôs tipo nenhum;
- **o evento volta ao MOD.** A sexta fala é o MOD reagindo ao estado da mídia,
  73 ms depois — o canal janela→MOD que a API ganhou nesta rodada;
- **a gravação no servidor persiste**: `mod_data.vezes` subiu a cada execução
  que completou o pedido (0 → 1 → 2 → 3).

**Não foi exercitado nesta sessão**, e o motivo é um só: digitar, arrastar e
apertar. A automação de acessibilidade do macOS recusa este processo
(`osascript é um acesso assistivo não permitido`), e sem ela não há como
sintetizar entrada na janela. Fica pendente, e com ele:

- a latência de interação medida na janela;
- o arraste e o desenho no WKWebView;
- a saída **durante** o carregamento e a reprodução, e a troca A→B;
- o impacto na voz sob carga.

Os quatro primeiros são cobertos por `bancada/regiao-do-mod.cjs`, que roda o
`ui/mods-regiao.js` de verdade num DOM mínimo — inclusive sair durante o
carregamento e durante a reprodução. É prova mais fraca que a nativa, e está
dita como mais fraca.

## Consumo

Sete amostras a cada cinco segundos, `footprint` para a memória da família de
processos e `ps` para o tempo de CPU acumulado. Mesma máquina, mesmo binário,
mesmo servidor; a única diferença é `enabled` na tabela `mods`.

| Fase | Footprint (mediana) | CPU somada na janela |
| --- | --- | --- |
| sem MOD | 252,3 MiB | 1,32 s em 30,0 s = 4,4% de um núcleo |
| fatia montada | — ver abaixo — | — ver abaixo — |

A fase «fatia montada» foi colhida **duas vezes**. A primeira não vale: o
processo de medição ficou suspenso entre amostras e a janela saiu com 1959 s em
vez de 30 s, o que dilui a taxa de CPU até ela não querer dizer nada. O arquivo
está preservado fora do registro para não ser lido por engano.

A segunda colheita não aconteceu: a execução seguinte esbarrou no defeito
aberto descrito abaixo, e medir uma fatia que não terminou de subir mediria
outra coisa. **Fica pendente**, e não estimada.

O que se pode dizer com o que foi medido: com a fatia montada, o footprint
mediano ficou em 259,9 MiB contra 252,3 MiB sem MOD — cerca de 7,6 MiB para um
executor QuickJS, uma região com nove nós e um som de 1644 bytes decodificado.
O número é de uma colheita cuja janela temporal não vale para CPU; para memória
ele continua sendo sete amostras da mesma fase, e é assim que deve ser lido.

## Dois defeitos encontrados aqui, e não em teste

### Uma promessa rejeitada sem tratamento morria calada — **consertado**

O MOD de referência chamava `SeeleMods.snapshot()` numa função `async` sem
`catch`. Quando o MOD subia antes de a sessão estar de pé, o `snapshot`
rejeitava e a função morria na segunda linha. No registro isso aparecia como
duas falas e silêncio; na tela, como um MOD que não desenha. Levei três
execuções para separar isso de um travamento.

O motor **sabe**: QuickJS rastreia rejeições sem tratamento. Ligar o rastreador
custou dez linhas, e o aviso passa pela mesma cota dos outros — um MOD que
rejeita num laço não enche o canal. O guarda está em
`uma_promessa_rejeitada_sem_tratamento_nao_morre_calada`, escrito antes do
conserto e visto reprovando.

O outro lado é do MOD, e o vetor de referência mostra a forma certa: cada passo
da subida responde por si, e a subida inteira tem um `catch`.

### Um pedido ao servidor pode ficar preso para sempre — **aberto**

Em três das seis execuções, o MOD mandou o pedido ao servidor e **nada voltou**:
nem resposta, nem erro. O `mod_data.vezes` não subiu, então o servidor nunca
executou o pedido; e o prazo de 15 segundos de `pedirAoServidor` não resgatou —
26 segundos depois da terceira fala não havia quarta, que é o que a rejeição
teria produzido.

Não é o defeito acima: com o rastreador ligado, nenhum `Falhou` foi emitido.
O MOD está genuinamente esperando.

### O que a instrumentação seguinte mostrou

`mod_request` passou a dizer, no Rust, todo pedido que sai da janela. Com ela:

- o pedido do MOD **nunca chega ao Rust**. Os únicos que aparecem têm `mod_id`
  vazio, e esses são o catálogo que a própria janela consulta — e eles
  continuam fluindo depois do travamento, então a ponte não está parada;
- o MOD para em **duas** falas, não três: ele não chega a pedir ao servidor.
  A segunda fala é `SeeleMods.snapshot()`, e é a resposta dela que não volta;
- com o rastreador de rejeições ligado, **nenhum `Falhou` é emitido**: não é
  uma promessa rejeitada em silêncio. O MOD está genuinamente esperando;
- só uma instância é ativada, então não é outra instância tendo tomado o lugar.

Isso fechou um caminho de silêncio que era real e que este defeito revelou:
`atenderOMod` tinha um `return` calado depois do `await` do retrato. Quando a
sessão andava no meio, o pedido saía sem resposta nenhuma — e
`SeeleMods.request` é uma promessa, que não rejeita sozinha. Recusar não é
admitir efeito; é o contrário dele. Está consertado, com guarda e reversão.

**Mas não é o que prende o MOD aqui**, e a execução depois do conserto
continua parando em duas falas. O que sobra, e que não isolei, é o
`invoke("snapshot")` não se resolvendo. Não vou escrever palpite: o que está
medido é que acontece, que é intermitente — três de seis execuções —, que o
prazo de 15 s não cobre, e que o pedido não sai da janela.

O registro está em `execucao-pedido-preso.log`.

**Isto bloqueia a medição da fatia ativa**, e provavelmente bloqueia a entrega:
um MOD que espera para sempre por uma resposta que não vem é uma falha de
recuperação, e a diretriz lista isso entre os bloqueios.
