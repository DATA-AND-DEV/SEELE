# 05 — Cliente TUI (`plug`)

O produto principal. Tudo o mais imita esta interface.

## Stack

`ratatui` + `crossterm`. Renderização a ~30 fps apenas quando há mudança (redesenho sob demanda, não loop cego). Suporte a truecolor com degradação para 256 e 16 cores.

## Layout principal

```
┌ SEELE ─────────────────────── 同期率 ─── 第3新東京市 ─────── 12:04:33 ┐
│ SERVER          │ VOICE_ROOMS / LINHAS       │ MENSAGENS                  │
│ ▸ Terceira Tó… │ ▼ VOICE_ROOM-01 CENTRAL    │ 12:01 ayanami              │
│   Geofront     │   ● ayanami    98%   │   verificando harmônicos   │
│   Matsushiro   │   ● shinji     71%   │                            │
│                │   ○ asuka    A.T.    │ 12:03 shinji               │
│                │ ▼ VOICE_ROOM-02 TESTE      │   sync caiu aqui           │
│                │ ─ LINHA #geral       │                            │
│                │ ─ LINHA #logs        │ ▸ _                        │
├────────────────┴──────────────────────┴────────────────────────────┤
│ SYNC 94% │ RTT 38ms │ JIT 12ms │ LOSS 0.2% │ OPUS 32k │ A.T. OFF   │
└────────────────────────────────────────────────────────────────────┘
```

Três painéis verticais mais uma barra de telemetria fixa. Larguras assimétricas e ajustáveis. A telemetria é permanente, não escondida em menu — é a diferença de caráter em relação a um cliente de chat comum.

## Modelo de interação

Modal, no espírito do Vim, porque o público é esse:

| Modo | Entrada | Comportamento |
|---|---|---|
| **Normal** | padrão | Navegação por teclas simples |
| **Inserção** | `i` ou Enter no campo | Digitação de mensagem |
| **Comando** | `:` | Comandos explícitos |
| **Busca** | `/` | Busca no histórico |

Atalhos essenciais no modo Normal:

```
h j k l / setas   navegar
Tab / Shift+Tab   alternar painel, adiante e para trás
Enter             inserir plug no VoiceRoom / abrir Linha
i                 escrever mensagem
Espaço (hold)     push-to-talk
m                 alternar A.T. Field (mudo)
d                 alternar surdo
g / G             topo / fim do histórico
/                 buscar no histórico
n / N             próxima / anterior ocorrência da busca
?                 ajuda
:q                sair do programa
```

`h` e `l` movem o **foco** entre os painéis e dão a volta nas pontas, como o
`Tab`; `j` e `k` movem a seleção dentro do painel focado e prendem no fim. A
diferença é deliberada: uma lista que dá a volta faz `j` e `G` significarem a
mesma coisa com frequência suficiente para nenhum dos dois ser confiável.

A busca é salto, não filtro. `/` reconstrói o casamento a cada tecla e o
contador `[1/3]` anda junto — é o retorno que diz se vale continuar
escrevendo. `Enter` confirma e mantém o destaque; `Esc` desiste e o apaga. A
ocorrência corrente acende com ênfase diferente das demais, e o contador é a
metade textual disso: sem ele o destaque seria informação só na cor, que esta
spec proíbe. O cursor dá a volta nas duas pontas, porque quem procura trata a
última ocorrência e a primeira como vizinhas.

Comandos: `:conectar <host>`, `:voice_room <nome>`, `:sync` (diagnóstico detalhado), `:audio` (dispositivos), `:tema`, `:sobre`, `:ejetar` (sair deste Server e escolher outro).

**Resolvido em M4, e eram duas causas independentes.** A colisão com digitação: PTT só no modo Normal, onde não há nada com que colidir (decisão D19). E uma que esta spec não previa: **a maioria dos terminais não reporta soltura de tecla**, então "segurar espaço" abre um microfone que nunca fecha. Onde o protocolo de teclado do Kitty existe, é segurar de verdade; onde não existe, a barra vira trava — aperta para abrir, aperta para fechar (ADR 0016). A barra de telemetria diz qual estado está valendo nos dois casos.

Tecla dedicada configurável não resolveria: o problema não é *qual* tecla, é que nenhuma tem soltura nesses terminais.

## Estados visuais que precisam existir

1. **Boot** — sequência de inicialização, três subsistemas reportando, barra de sincronização subindo. Dura o tempo real da conexão; se conectar em 200 ms, não inventar espera artificial. Animação decorativa que atrasa o usuário é falha de design.
2. **PADRÃO: LARANJA** — conectado, não autenticado.
3. **PADRÃO: AZUL** — operação normal.
4. **Falando** — destaque no roster, indicador de nível.
5. **Bateria interna** — desconectado, contagem 04:59 regressiva, interface esmaecida mas legível, tentativas listadas.
6. **Alerta** — banner 警告 para menção direta ou evento crítico. Ocupa as linhas que o texto pedir, **até quatro**, e essas linhas saem mesmo das da conversa — os painéis têm piso de três linhas e a barra de telemetria fica fora da conta. Cresce porque um veredito de convite carrega duas impressões digitais de 64 caracteres, e em 80 colunas elas não cabem numa linha só: mostrar metade de uma comparação é o mesmo que não mostrar nenhuma. Para em quatro porque o texto do alerta pode vir do operador do outro lado — 512 bytes sem filtro de quebra de linha —, e uma banda que se dimensiona ao que o servidor mandar é o servidor decidindo quanto da conversa você enxerga. O que passar disso é cortado com `…`, e a dica `[enter]` do alerta bloqueante nunca é o que se corta — quando ela não cabe na última linha, a linha que ela toma sai marcada como qualquer outro corte.

### A tela de conexão não é o sétimo

Os seis acima descrevem uma **sessão**. `plug` sem argumento nenhum abre antes
disso uma tela de conexão — Servers visitados, endereço novo, colar convite,
hospedar aqui — que não tem roster, telemetria nem Taxa de Sincronização.
Encaixá-la no mesmo enum custaria campos vazios nos outros seis, então ela vive
fora, em `seele-tui::selecao`, e some quando a conexão começa.

**Qualquer flag pula a tela.** `plug --server casa:8383` conecta direto, sem
uma tecla no caminho. Um menu obrigatório entre a intenção e o resultado é o
oposto do que este cliente promete a quem já sabe o que quer.

A flag pula a tela na **entrada**, e só. Um `--server` que não responde não
imprime o motivo e devolve o terminal: ele mostra o motivo, e depois a tela de
conexão, onde espera por alguém. Quem chama `plug` de dentro de um script tem
de contar com isso — é um programa interativo do começo ao fim, e não um
comando que termina sozinho quando dá errado.

Os servidores visitados ficam num arquivo à parte dos pins, e a separação é
deliberada: o arquivo de pins decide se um servidor é o mesmo de ontem e por
isso é curto e legível a olho. Apelido e último VoiceRoom são conveniência — um pode
ser apagado sem consequência, o outro não.

### Sair do programa é uma coisa; sair do servidor é outra

`:q`, `:quit`, `:sair` e Ctrl-C fecham o cliente. **Nada mais fecha.**

Toda outra forma de uma sessão acabar volta à tela de conexão. `:ejetar` volta
direto, porque quem ejetou já sabe por quê. As outras mostram antes o motivo e
esperam ser lidas: o servidor que não atendeu, o convite cuja impressão digital não
bate com a do servidor que respondeu, o servidor que desconectou — expulso, barrado,
lotado —, a bateria interna que esgotou os cinco minutos.

Em todos os casos o enlace e o áudio são derrubados de verdade antes da volta, e
com `--hospedar` o servidor daqui também: a tela de conexão não tem roster,
telemetria nem som.

Um cliente que some no instante em que perde o enlace leva o motivo junto, e
some justamente de onde a próxima tentativa começaria. Fechar o programa é um
pedido; perder a conexão não é.

### Hospedar sem daemon

`plug --hospedar` sobe um servidor dentro do próprio processo e entra nele; o link
de convite aparece de saída numa sobreposição de largura inteira, e `:convite`
o traz de volta. Não substitui o `seeled`: este servidor morre quando o cliente
fecha, que é o certo para "estou hospedando uma conversa" e errado para
"mantenho um servidor no ar".

## Restrições de renderização

- Tudo alinhado a células. Bordas com box-drawing (`│ ─ ┌ ┼ ╮`).
- Barras com blocos (`█ ▓ ▒ ░` e `▁▂▃▄▅▆▇` para nível de áudio).
- Sem imagem, sem fonte customizada. Ênfase disponível: cor, negrito, inverso, sublinhado.
- Kanji ocupa duas células — calcular largura com `unicode-width`, nunca com `.len()`. Isso vai quebrar o layout se esquecido.
- Terminal mínimo suportado: 80×24. Abaixo disso, degradar para painel único com aviso.

## Acessibilidade

- Modo alto contraste e modo sem cor (só forma e texto) — daltonismo é comum no público e a paleta depende muito de vermelho/verde.
- Nenhuma informação transmitida **só** por cor: a Taxa de Sincronização é sempre acompanhada do número; A.T. Field tem marcador textual além da cor.
- Respeitar `NO_COLOR`.
- **[EM ABERTO]** Leitor de tela em TUI é limitado. Investigar viabilidade mínima.

## Critérios de aceite

- RSS abaixo de 60 MB em operação normal.
- Sem tremulação ao redimensionar.
- Funciona por SSH em terminal de 16 cores sem perder informação.
- Do lançamento até pronto para falar em menos de 1,5 s.
