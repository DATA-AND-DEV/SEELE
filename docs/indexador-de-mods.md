# `mods.seele.app.br` — o indexador, e por que ele é arquivo parado

> Decisão: [ADR 0044](adr/0044-mods-o-produto-base-tem-regras-e-um-mod-nao.md).
> Esta página é **como construir**, não se construir.

## Cloudflare Pages serve, e serve melhor que um servidor

Sim. E não por conveniência — porque **Pages não sabe fazer o que o ADR 0044
proíbe.**

O ADR decidiu catálogo estático e busca no cliente, com este argumento:

> Com API, o indexador aprende cada termo que alguém digitou; com catálogo,
> aprende que alguém buscou o catálogo.

Hospedagem estática não é uma escolha compatível com essa regra: é a regra
virando impossibilidade. Não há como acrescentar uma rota de busca sem trocar de
hospedagem, e trocar de hospedagem é uma decisão visível. É a mesma propriedade
que o ADR 0026 gosta no release do GitHub — «não há serviço a hospedar para isso
valer».

### As três ressalvas, ditas antes de você subir

**1. A Cloudflare vê quem pediu o quê.** O ADR já escreve que «quem serve os
bytes vê quem os pediu», e escreve que o endereço fixo piorou isso ao tirar o
espelho. O que ele **não** dizia é que quem vê passa a ser uma empresa
americana, num produto cujo argumento é não ter serviço no meio.

Não há como zerar isso com Pages. O que dá para fazer, e deve ser feito:
**não ligar Web Analytics, não ligar Logpush, e não pôr nada de análise no
projeto.** Assim os registros ficam sendo só os que a Cloudflare guarda para si,
em vez de os que nós pedimos que ela guardasse para nós. A diferença é pequena e
é real.

Isto merece uma linha nas notas de release quando o indexador for ao ar. Um
produto que se vende como auto-hospedado e passa a depender de uma CDN precisa
dizer isso em voz alta em vez de deixar descobrir.

**2. Os tetos do Pages são o teto do catálogo.** 20 000 arquivos por projeto e
25 MiB por arquivo. Com o desenho abaixo — arquivos soltos, sem pacote — um MOD
médio ocupa uns 4 arquivos por versão, então o teto real é da ordem de **5 000
versões publicadas**. É muito, e não é infinito: quando chegar perto, a saída é
um segundo projeto por letra inicial de autor, e ela não exige mudar nada no
cliente.

**3. Cache é a parte que erra.** Uma revogação que fica presa num nó da CDN é
uma versão furada continuando a rodar. A seção «Cabeçalhos» abaixo é a que não
pode ser copiada por cima sem ler.

E uma que **não** é ressalva: **CORS não se aplica.** Quem busca é um cliente
HTTP em Rust, não um navegador. Não perca tempo com isso.

## A árvore do site

```
/
├── catalogo.json              o índice inteiro
├── catalogo.json.minisig      assinatura dele
├── revogacoes.json            o que foi retirado
├── revogacoes.json.minisig    assinatura dela
├── chave.pub                  a pública, por conveniência humana
├── _headers                   as regras de cache
└── mods/
    └── <autor>/<nome>/<versao>/
        ├── mod.json
        ├── cliente/main.js
        └── servidor/main.js
```

**Arquivos soltos, e não um `.zip`.** Três motivos, e o terceiro decide:

- não há formato de arquivo a escolher, nem extração, nem *zip slip* — a classe
  de defeito que a pasta do MOD já custou um guarda para impedir no disco;
- o `content_hash` que o produto já tem **já** trabalha sobre pares
  `(caminho, bytes)`, então o cliente confere o conjunto sem código novo;
- e o hash cobre o **conjunto**: um catálogo adulterado que omitisse um arquivo
  mudaria o hash, e o cliente recusaria. Um pacote precisaria de uma segunda
  conferência para dizer a mesma coisa.

O custo é uma requisição por arquivo. Com HTTP/2 e MODs de poucos kilobytes, é
troco.

**O caminho carrega a versão, e isso não é organização: é o que torna cada
arquivo imutável.** `mods/seele/rpg/1.0.0/cliente/main.js` nunca muda de
conteúdo. Uma versão nova é um caminho novo. É o que permite o cache eterno da
seção seguinte, e é o que faz «o MOD que você baixou é o MOD que revisamos»
valer sem nenhum mecanismo.

## `catalogo.json`

```json
{
  "esquema": 1,
  "gerado_em": 1757100000,
  "mods": [
    {
      "id": "seele/rpg",
      "autor": "seele",
      "nome": "rpg",
      "titulo": "Salas de RPG",
      "resumo": "Fichas, dados e salas por mesa.",
      "repo": "https://github.com/seele/mod-rpg",
      "oficial": true,
      "versoes": [
        {
          "versao": "1.0.0",
          "api": 1,
          "publicado_em": 1757000000,
          "hash": "9f2c…",
          "arquivos": [
            "mod.json",
            "cliente/main.js",
            "servidor/main.js"
          ]
        }
      ]
    }
  ]
}
```

- **`hash`** é o `seele_proto::mods::content_hash` do conjunto, em hexadecimal
  minúsculo. É o mesmo número que o servidor anuncia e que o cliente confere; é
  também o número que uma pessoa compara a olho, que é por que ele é texto.
- **`arquivos`** é a lista exata a buscar. O cliente não descobre nada: ele lê a
  lista, busca aquilo, e confere o hash do conjunto.
- **`oficial`** é **conveniência de tela e não prova.** Quem prova é a assinatura
  `minisign` do MOD, conferida contra a chave embutida no app. O ADR 0026 já
  escreveu o porquê: se o indexador mentir, um campo mente junto; uma assinatura
  não.
- **`api`** por versão, porque é o que permite ao catálogo se filtrar: o cliente
  sabe qual API ele oferece e não mostra o que vai recusar.

## `revogacoes.json`

```json
{
  "esquema": 1,
  "gerado_em": 1757100000,
  "mods": [
    {
      "id": "alguem/ruim",
      "versao": "1.2.0",
      "motivo": "credencial-vazada",
      "desde": 1757050000,
      "corrigido_em": "1.2.1"
    }
  ],
  "versoes_do_produto": [
    {
      "versao": "0.11.2",
      "motivo": "leitura-de-disco-fora-da-pasta",
      "desde": 1757050000,
      "corrigido_em": "0.11.3"
    }
  ]
}
```

**Uma lista e não duas.** O ADR 0044 pede remoção de MOD que alcance quem já
baixou; o ADR 0045 pede revogação de versão do produto. São a mesma peça, e duas
listas seriam duas chances de esquecer uma.

**`motivo` é um identificador de uma lista fechada, e não uma frase.** Quem
escreve a frase é o `ui/frases.js` — ADR 0012, e vale aqui como vale em todo
lugar: o núcleo nunca formata mensagem.

**`corrigido_em` é o que transforma a recusa em conserto.** Uma recusa que só diz
«não» manda a pessoa procurar; uma que diz para onde ir resolve. É o mesmo
critério do `specs/02-protocolo.md` sobre toda razão ser específica.

## `_headers` — a parte que não se copia sem ler

```
/mods/*
  Cache-Control: public, max-age=31536000, immutable

/catalogo.json
  Cache-Control: public, max-age=300

/catalogo.json.minisig
  Cache-Control: public, max-age=300

/revogacoes.json
  Cache-Control: public, max-age=60, must-revalidate

/revogacoes.json.minisig
  Cache-Control: public, max-age=60, must-revalidate
```

O raciocínio, porque os três números são decisões diferentes:

- **`/mods/*` é eterno** porque o caminho carrega a versão e o conteúdo nunca
  muda. Um MOD baixado uma vez não é baixado de novo, e a CDN faz o trabalho
  todo.
- **O catálogo em 5 minutos** é o atraso máximo entre você aprovar um MOD e ele
  aparecer para alguém. Curto o bastante para não parecer quebrado, longo o
  bastante para a CDN valer.
- **As revogações em 1 minuto, com `must-revalidate`.** É o número que decide
  quanto tempo uma versão furada continua rodando depois de você a retirar. Um
  minuto é caro em requisições e barato em tudo o mais.

**A assinatura tem de ter o mesmo cache do arquivo que ela assina.** Um catálogo
novo com uma assinatura velha em cache é um cliente recusando um catálogo
legítimo — a falha mais difícil de diagnosticar deste desenho inteiro, porque
ela parece adulteração.

## Como publicar, e onde a sua revisão entra

```
repositório SEELE-MODS (público)
  └── mods/<autor>/<nome>/<versao>/…      ← o MOD, como fonte
  └── ferramentas/gerar.py                ← monta catalogo.json e assina
  └── publicado/                          ← o que o Pages serve
```

1. **O autor abre um PR** acrescentando `mods/<autor>/<nome>/<versao>/`. Nunca
   editando uma versão que já existe — é a mesma regra append-only das migrações
   e do `api/`, e o `gerar.py` a cobra.
2. **Você revisa no PR.** É onde a exigência de «revisão de cada versão» do ADR
   0044 mora, e o PR é o formato certo: fica registrado, tem diff, e a versão
   seguinte mostra só o que mudou.
3. **Você roda `gerar.py` na sua máquina**, que monta o catálogo, calcula os
   hashes e **assina com a chave privada de MOD**.
4. **Commit do `publicado/`**, e o Pages serve aquilo.

**A chave privada nunca entra na Cloudflare, nem em CI.** Assinar localmente e
comitar o `.minisig` é o que mantém a propriedade que o ADR 0026 mais preza: a
chave que autoriza é a única coisa que um invasor não alcança pela rede. Um
segredo em CI é um segredo em máquina de terceiro; este não precisa ser.

O preço é que publicar exige você na frente do computador. Para o volume de um
indexador que passa por revisão humana de qualquer jeito, isso não é o gargalo.

**A chave de MOD é a segunda, e separada da do atualizador.** ADR 0044: uma chave
que atesta duas coisas deixa as duas se passarem uma pela outra, e a do 0026
autoriza instalar programa.

## O que o cliente faz, na ordem

Escrito aqui porque é o que o site tem de suportar, e nada além:

1. `GET /catalogo.json` e `GET /catalogo.json.minisig`;
2. confere a assinatura contra a chave embutida — **se falhar, para**, e não
   tenta de novo com outro endereço;
3. busca **localmente**, no que baixou;
4. `GET /revogacoes.json` (+ assinatura) ao hospedar e ao entrar — nunca ao
   abrir o app, ADR 0026;
5. para o MOD escolhido, `GET` de cada caminho de `arquivos`;
6. calcula o `content_hash` do conjunto e compara com o do catálogo — **se não
   bater, não instala**, e diz qual foi o número esperado e qual veio;
7. confere a assinatura do MOD, se ele se diz oficial.

Nenhum passo acima precisa de nada além de arquivos parados. É a prova de que a
hospedagem está certa.

## As três falhas, e o que a tela diz em cada uma

| o que aconteceu | o que a pessoa lê |
|---|---|
| o catálogo não veio | «`mods.seele.app.br` não respondeu» — e, se ela estava entrando num servidor com MOD, que **é por isso** que ela não entra |
| a assinatura não confere | «o catálogo não é nosso» — sem oferecer «tentar assim mesmo», que o ADR 0029 recusa nominalmente |
| o hash não bate | o número esperado e o que veio, lado a lado, porque é a única forma de alguém descobrir *onde* foi adulterado |

A primeira é a que vai acontecer, e é a que o ADR 0044 escreveu como custo:
**LAN sem internet não entra em servidor com MOD.** A tela tem de ligar as duas
coisas na mesma frase, ou quem hospeda vai culpar o próprio roteador.

## O que fica sem saída

**A Cloudflare vê quem pediu o quê**, e não há desenho que tire isso de uma CDN.
Minimizado por não coletarmos nada nós mesmos; verdade mesmo assim.

**Publicar depende de uma pessoa com a chave.** É deliberado, e significa que o
indexador para quando essa pessoa para. Com revisão humana obrigatória, ele já
parava; a chave só torna isso explícito.

**5 000 versões é um teto real**, e a saída — dividir em projetos — é trabalho de
infraestrutura no dia em que doer.

**Um MOD retirado continua no disco de quem já baixou.** A revogação impede
instalar e impede hospedar; ela não apaga o que já está numa máquina. Não existe
versão disto que apague, e prometer que existe seria pior que dizer que não.
