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
mesmo servidor; a única diferença entre as fases é `enabled` na tabela `mods`.

A família é escolhida por regra, e não a olho: o processo do aplicativo e os
auxiliares do WebKit **mais novos que ele**. Uma primeira tentativa pegou um
auxiliar de outro aplicativo e devolveu 145 MiB contra 252 MiB da fase
anterior — números que não comparavam nada. As duas fases abaixo foram colhidas
seguidas, com a mesma regra e a mesma janela.

| Fase | Footprint (mediana) | CPU na janela de 30 s |
| --- | --- | --- |
| sem MOD | 246,7 MiB | 1,79 s = 6,0% de um núcleo |
| fatia montada | 267,9 MiB | 1,84 s = 6,1% de um núcleo |

**+21,2 MiB e +0,1 ponto de CPU** para um executor QuickJS de pé, uma região de
nove nós montada e um som de 1644 bytes decodificado e parado.

A CPU praticamente não muda porque a fatia, depois de montar, **não faz nada**:
não há temporizador, não há laço, e o MOD só acorda por evento. É o resultado
que o desenho previa, e é também o motivo pelo qual ele não diz nada sobre
custo **sob interação** — isso continua pendente, pelo impedimento de
automação.

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
continua parando. Duas hipóteses foram medidas e **mortas**:

- **não é o retrato demorando.** `snapshot` foi instrumentado na entrada e na
  saída: ele responde em microssegundos, dezenas de vezes por minuto;
- **não é entrega recusada.** `mod_nativo_entregar` passou a dizer toda recusa
  com o tamanho junto, e nenhuma apareceu.

O que ficou, e que é uma afirmação bem mais estreita do que a de antes:

> O pedido do MOD sai do executor — a fala aparece no registro — e **nunca vira
> `invoke("mod_request")`**. As consultas de catálogo da própria janela, que
> passam pela **mesma função** `pedirAoServidor`, continuam saindo a cada quatro
> segundos, antes e depois. Então a função é alcançável e a ponte está viva.

Ou seja: entre `atenderOMod` receber a fala e `pedirAoServidor` chamar o
`invoke`, alguma coisa desiste sem dizer. As saídas conhecidas dessa função —
geração morta, oito pedidos em voo, carga grande demais — todas **lançam**, e um
lançamento viraria resposta de recusa e uma quarta fala. Não há quarta fala.

### O que foi feito a respeito

A janela ganhou um caminho de registro — `registrar_da_janela` —, porque ela não
tinha nenhum: todo o caminho de um pedido era observável no Rust, e o pedaço que
roda na janela era um vão silencioso no meio dele. Com ele, cada porta de recusa
de `pedirAoServidor` diz qual fechou, o `catch` de `atenderOMod` diz o que
falhou, e o `return` de «esta instância já não é a carregada» deixou de ser mudo.

A bomba também passou a dizer o **descarte**. A linha «fala de MOD nativo» sai
antes da decisão, então «chegou» e «foi guardada» eram indistinguíveis; uma fala
descartada ali deixa o MOD esperando para sempre a resposta que ela carregava, e
não deixava rastro. Agora diz qual dos dois motivos foi.

E a colheita diz quantas levou e quantas ficaram, que é o outro lado da mesma
linha.

**O defeito não reproduziu na execução seguinte**, e eu parei aí, tendo tirado
o silêncio dos caminhos sem chegar à causa.

### E a causa era a colheita — corrigido

A revisão seguinte a reproduziu e a nomeou, na camada JS e sem tocar no
transporte:

1. o Rust atende uma colheita vazia, e a resposta ainda está a caminho do
   JavaScript;
2. uma mensagem nova entra e produz o aviso dela. A janela o recebe com
   `colhendo = true`, e o ignorava;
3. a resposta vazia antiga chega, o coletor sai, e a mensagem nova fica retida
   sem nenhum aviso que a vá buscar.

Bate com o que o registro mostrava: a terceira fala chegava à bomba e a
colheita seguinte levava zero. Uma marca de «colher de novo» preserva o aviso
recebido durante a espera, sem consulta concorrente e sem varredura ociosa. O
caso entrou na bancada como `avisoDuranteColheitaVaziaNaoSePerde`; reverti a
correção e ele reprova com as duas frases certas.

**Confirmado no nativo**: com o aplicativo recompilado, **seis execuções de
seis** subiram a fatia inteira — cinco falas, mídia servida, zero descartes.
Antes, três de seis travavam. Uma execução boa não provaria nada numa falha
intermitente; seis seguidas, contra três de seis, provam.

O registro está em `execucao-pedido-preso.log`.

**Isto bloqueia a medição da fatia ativa**, e provavelmente bloqueia a entrega:
um MOD que espera para sempre por uma resposta que não vem é uma falha de
recuperação, e a diretriz lista isso entre os bloqueios.
