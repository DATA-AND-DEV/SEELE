# 0006 — `seele://` para convidar alguém com um link

Status: aceito (era `proposto` desde M2)

> **Vocabulário.** Esta página é anterior ao [ADR
> 0035](0035-o-codigo-deixa-de-falar-evangelion.md) e diz `Dogma` onde o
> produto hoje diz **servidor** e `Cage` onde diz **sala de voz**. O texto fica
> como foi escrito; **o esquema da URI, não** — ele é endereço, não registro, e
> um parâmetro errado aqui vira um link que não faz o que promete. O `cage=` do
> exemplo virou `room=`, com a consequência anotada na lista.

Contexto: entrar num Dogma exigia passar endereço, porta e — depois do ADR 0021 — um segredo, cada um por um canal, e ainda pedir que a pessoa conferisse a impressão digital do certificado por fora. Ninguém confere. Um primeiro contato TOFU que na prática é cego não é confiança na primeira vez, é aceitar qualquer um.

Decisão: um esquema de URI que carrega o que faz falta.

```
seele://servidor.exemplo:8383/?alt=<outros>&fp=<impressão>&convite=<token>&room=<n>
```

- **endereço** — obrigatório.
- **`alt`** — os outros endereços do mesmo Dogma, separados por vírgula, na
  ordem em que se tenta. Acrescentado em 2026-08-17; ver "O endereço nunca
  podia ser um só", no fim.
- **`fp`** — a impressão digital do certificado. É o principal motivo disto existir: o cliente compara antes de fixar, então o primeiro contato passa a ser verificado, e um servidor no meio do caminho não passa.
- **`convite`** — token de uso único do ADR 0021.
- **`room`** — em qual sala de voz entrar ao conectar. **Chamava-se `cage=` até
  2026-08-25**, e o [ADR 0035](0035-o-codigo-deixa-de-falar-evangelion.md)
  registra o que a troca custou: pela regra logo abaixo, um link antigo com
  `cage=` não é recusado — o parâmetro é ignorado, e a sala escolhida se perde
  em silêncio.

O `seeled convite` imprime a URI pronta, já com a impressão digital e o endereço de rede da máquina.

**A senha do Dogma não viaja no link, e isso é decisão e não esquecimento.** Uma senha vale para sempre e para todo mundo; um link acaba em histórico de terminal, backup de conversa e captura de tela. O convite existe exatamente para ocupar esse lugar, e é descartável por construção. Quem usa senha digita a senha. Há um teste que falha se alguém acrescentar o campo.

Tudo é validado antes de virar `Convite`: o endereço aceita só o que aparece num `host[:porta]`, a impressão digital tem de ser 64 dígitos hexadecimais, e o token tem de estar no alfabeto de convites. Este texto chega colado de uma conversa e termina num `connect` — a validação é a porta de entrada e está testada com entradas hostis.

Parâmetro desconhecido é ignorado em vez de recusado, para que dê para acrescentar um campo depois sem que cliente velho recuse link novo.

Alternativas:

1. **Só endereço e porta, como sempre.** Mantém o primeiro contato cego, que é o problema.
2. **Um link `https://` para uma página que redireciona.** Precisaria de um servidor nosso no meio, o que contraria a coisa toda.
3. **Registrar o esquema no sistema operacional**, para clicar e abrir. Vale a pena e não está feito: exige mexer em `Info.plist`, registro do Windows e `.desktop`, e um esquema clicável é uma superfície nova — um link malicioso passa a poder iniciar uma conexão sem que ninguém digite nada. Quando for feito, o cliente deve **perguntar antes de conectar**.

Consequências:

- Um convite vira uma linha que se cola numa conversa, e quem recebe não precisa conferir nada por fora.
- **Uma impressão digital truncada ou errada é recusada na leitura**, e não silenciosamente ignorada: aceitar uma impressão parcial seria pior que não ter nenhuma, porque o cliente compararia e passaria achando que verificou.
- O link revela que existe um Dogma naquele endereço. Para quem já tem o endereço, não é novidade.

Custo de reverter: **baixo**. Um módulo em `seele-proto` e uma opção `--url` no cliente.

## O endereço nunca podia ser um só

Escrito em 2026-08-17, depois de um defeito de campo. Este ADR dizia "endereço —
obrigatório" e parava aí, no singular, e o singular estava errado desde sempre:
**uma máquina tem vários endereços, e nenhum deles serve para todo mundo.**

- O da rede de casa é o único que serve para quem está na sala ao lado, e não é
  alcançável de fora.
- O público que o roteador abriu (degrau 3 do ADR 0022) serve de fora e quase
  nunca volta para dentro da própria casa: a maioria dos roteadores domésticos
  não faz *hairpin*.
- O de uma VPN serve para quem estiver na mesma VPN, e para mais ninguém.

Enquanto o link levava um endereço só, escolher qualquer um deles perdia alguma
situação — e o ADR 0022, ao mandar pôr no convite o endereço do **degrau mais
alto**, escolheu justamente o que perdia o caso mais comum de todos. Foi assim
que 0.5.0 quebrou "os dois estão na mesma rede", que era o único caso que sempre
tinha funcionado.

### A forma, e as quatro combinações de versão

`alt=` carrega os endereços restantes, separados por vírgula, cada um validado
exatamente como o principal — este texto termina num `connect` igual ao outro. O
máximo é quatro endereços contando o principal: cada um custa a quem recebe uma
tentativa com prazo antes de a sala abrir.

**O primeiro endereço é o da rede local**, e essa é a decisão que faz a
compatibilidade funcionar em vez de ser declarada:

| | Convite antigo | Convite novo |
|---|---|---|
| **Cliente antigo** | como sempre foi | lê `alt` como parâmetro desconhecido e o ignora; usa o endereço da rede local, que é o comportamento de antes da 0.5.0 |
| **Cliente novo** | uma lista de um item, e o caminho de antes: sem prazo novo, sem tentativa extra | tenta um de cada vez, na ordem, e para no primeiro que atender |

A regra que faz a linha de cima funcionar é a que este ADR já tinha: *parâmetro
desconhecido é ignorado em vez de recusado*. Ela foi escrita para poder
acrescentar um campo depois, e é a primeira vez que ela paga.

### A ordem é conteúdo, e não arrumação

Rede local, depois endereço global, depois a porta do roteador, e túnel por
último. Pôr o público na frente faria quem está na mesma casa esperar o prazo
inteiro de um caminho que não volta — o custo cairia inteiro sobre o caso mais
comum, para beneficiar o mais raro.

Em série, e não em corrida: cada aperto de mão fixa chave (ADR 0003), gasta o
convite de uso único (ADR 0021) e aparece no log de quem hospeda. Abrir três
para descartar dois seria pagar isso três vezes, e no caso comum o primeiro
responde antes de o segundo ser cogitado.
