# 0006 — `seele://` para convidar alguém com um link

Status: aceito (era `proposto` desde M2)

Contexto: entrar num Dogma exigia passar endereço, porta e — depois do ADR 0021 — um segredo, cada um por um canal, e ainda pedir que a pessoa conferisse a impressão digital do certificado por fora. Ninguém confere. Um primeiro contato TOFU que na prática é cego não é confiança na primeira vez, é aceitar qualquer um.

Decisão: um esquema de URI que carrega o que faz falta.

```
seele://dogma.exemplo:8383/?fp=<impressão>&convite=<token>&cage=<n>
```

- **endereço** — obrigatório.
- **`fp`** — a impressão digital do certificado. É o principal motivo disto existir: o cliente compara antes de fixar, então o primeiro contato passa a ser verificado, e um servidor no meio do caminho não passa.
- **`convite`** — token de uso único do ADR 0021.
- **`cage`** — em qual Cage entrar ao conectar.

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
