# A superfície que um MOD enxerga

Um arquivo por versão, e **nenhum deles é editado depois de publicado**.

`api/congeladas.sha256` guarda o SHA-256 de cada um, e o teste
`a_api_publicada_nao_se_edita` (em `crates/seele-conformance`) reprova o
`cargo test` — e com ele o portão da publicação — quando os bytes de uma versão
listada mudam ou quando aparece uma `vN.json` fora da lista. Mudar o que uma
versão promete é publicar a seguinte, e nunca editar esta — a v3 e a v4,
editadas antes de o guarda existir, ficaram congeladas como estão, com a
história no comentário da lista.

## O que muda para quem escreve MOD nesta versão

**Nada disto está num `vN.json`.** A superfície congelada é a mesma, e um MOD
das APIs 3, 4 e 5 carrega como carregava (`APIS_ACEITAS = [5, 4, 3]`, em
`crates/seele-proto/src/mods.rs`, na v0.15.0 e aqui). O que muda, na versão
depois da 0.15.0, é o executor da metade de janela e o som, e quem escreve MOD
precisa saber disso para o MOD rodar igual nas duas.

- **`console` existe, em qualquer API.** `console.log`, `info`, `warn`,
  `error` e `debug` — e `dir`, `table`, `trace` e o `assert` que falha —
  viram uma linha no `seele.log` da máquina que roda o MOD, com o id dele num
  campo próprio (`mod_id=`); `group`, `time`, `count` e `clear` não fazem
  nada. O `debug` não é gravado pelo filtro padrão do `seele.log`. Como ler
  essas linhas está em
  [`docs/como-se-faz-um-mod.md`](../docs/como-se-faz-um-mod.md), «Quando o MOD
  não faz o que devia».
- **Na 0.15.0 o `console` não existe.** Lá, `console.warn(…)` é um
  `ReferenceError`, e um MOD que o chame fora de um `try` quebra, na mesma
  sala, para quem não atualizou. Quem precisa rodar nas duas versões escreve:

  ```js
  if (typeof console !== "undefined") console.warn("o retrato não veio");
  ```

- **O erro que o MOD não pegou vai ao `seele.log`, em ERROR**, com o texto e a
  primeira linha da pilha: a exceção no topo do código, num temporizador e num
  ouvinte de evento, e a promessa rejeitada sem tratamento («promessa
  rejeitada sem tratamento: …»), dita no fim da volta e só se ninguém a pegou.
  A gestão de MODs continua dizendo «falhou» como antes.
- **O som do pacote toca por WebAudio**, sem `<audio>` e sem afrouxar a
  política da janela, e o contrato de `tocando` ficou escrito:
  - **`tocando` vale quando muda.** Um som que a pessoa pausou não volta
    sozinho no redesenho seguinte que o declare `tocando: true`.
  - **A exceção é o som que terminou:** `tocando: true` sobre um som cujo
    último aviso foi `terminou` toca de novo, do começo, como o `<audio>`
    fazia até a 0.15.0. Repetição explícita (`repetir`) é decisão da API 6.
  - **No fim, o MOD recebe só `terminou`.** O `<audio>` dizia `pausada`
    antes dele.
  - **O botão é do produto.** No lugar dos controles do `<audio>`, o produto
    desenha um TOCAR/PAUSAR ao lado do som (num cartão, nenhum).
  - **Fora da tela, a declaração não liga o som.** Um `tocando: true` num som
    cujo lugar saiu da tela — uma página fechada, um cartão que o SEELE não
    desenha agora — é recusado (`recusada`, com o motivo «o lugar deste som
    saiu da tela…»), e um som que tocava ali é pausado (`pausada`).
- **O som decodificado tem teto.** O WebAudio guarda o som inteiro em
  `float32`, e por isso há um teto além do de bytes: 64 MiB de som de pé por
  região, por superfície, por contribuição e nos cartões de um MOD — pouco
  menos de três minutos de som estéreo a 48 kHz, ou o dobro em mono. Acima
  dele, o som chega ao MOD como `recusada`, com os números no `porque`, como a
  mídia acima do teto de bytes.

## Publicar a versão seguinte

O guarda trata como publicada toda versão até `MOD_API_VERSION`
(`crates/seele-proto/src/mods.rs`), e não só as que já saíram numa release: é
esse número que diz o que o build oferece. Por isso a ordem dos passos importa.

1. `api/vN.json` e a linha dela em `api/congeladas.sha256` entram no mesmo
   commit. Uma `vN.json` fora da lista reprova, e a reprovação dá a linha.
2. Enquanto `MOD_API_VERSION` for menor que N, a vN está em construção: a linha
   acompanha cada edição, e a reprovação dá a linha nova.
3. O `cargo xtask check-api` cobra `moments` e `eventos` desde que o arquivo
   existe, e não desde que a versão sai: a vN que não os escreve, ou que
   promete um nome sem despachante, reprova ainda em construção.
4. `MOD_API_VERSION` e `APIS_ACEITAS` sobem por último, quando a vN está pronta
   para sair, porque daí em diante ela está congelada, mesmo antes da release.
   Uma edição depois disso reprova como edição de versão publicada, e a mudança
   vai para a versão N+1. Subir antes faz de cada ajuste que a vN ainda pedir
   uma versão nova.

## Por que congelado

O [ADR 0045](../docs/adr/0045-mods-o-produto-base-tem-regras-e-um-mod-nao.md)
decidiu que a API de MOD é uma **fachada** e não uma projeção do protocolo. A
diferença é a que importa quando alguém renomeia um campo:

- projeção: o nome muda por dentro, a API muda junto, **todo MOD quebra**;
- fachada: o nome muda por dentro, o build fica vermelho, e o conserto é o
  **mapeamento** — o nome que o MOD escreve nunca mudou.

`cargo xtask check-api` é quem cobra isso, e a pergunta que ele faz é **«todo
nome deste arquivo ainda aponta para alguma coisa?»**. Nunca o contrário: um
guarda na outra direção arrastaria a API atrás do protocolo, e seria a projeção
de novo com outro nome.

É a mesma disciplina das migrações do servidor — *«Append only once shipped»*
mais `no_migration_contains_a_down_step` —, aplicada a um segundo lugar onde ela
vale pelo mesmo motivo.

## Quando congelar passa a doer

**Quando existe MOD de terceiro no mundo**, e não antes. Enquanto o indexador
não publica, este arquivo ainda se mexe. Depois, não: editar uma versão
publicada é quebrar o MOD de todo mundo, que é a razão pela qual esta porta não
fecha.

É isto que torna «liberdade total na v1» — o pedido do dono — construível sem
adivinhação: a superfície cresce enquanto ninguém depende dela, e fecha antes de
publicar em vez de antes de existir.

## Os quatro blocos

- **`reads`** — o que um MOD lê do domínio.
- **`moments`** — quando ele é chamado. São eventos do `ServerMessage`, com os
  nomes que o fio já usa: um relatório de defeito que diz `MessageReceived`
  acha o mesmo nome no protocolo, sem intermediário. **Da v1 à v5, só 5 dos 21
  momentos são entregues.** A v1 lista 21, a v2 à v5 os herdam pelo `extends`,
  e `momento_de` (`crates/seele-server/src/mods/despacho.rs`), o único
  despachante de momentos do servidor, entrega só `PersonJoined`, `PersonLeft`,
  `MessageReceived`, `MessageEdited` e `MessageRemoved`. Os outros 16 nunca
  foram entregues: um MOD que espera por `ChannelCreated` espera para sempre,
  sem erro. A v1 não se edita, e por isso a v6 lista só o que é entregue: da
  API 6 em diante, `moments` e `eventos` são escritos por inteiro em cada versão
  e não se herdam nem se somam pelo `extends`, e o que a versão lista é tudo o
  que ela promete. O `cargo xtask check-api` reprova a versão que não os escreve
  e a que lista um nome sem despachante.
- **`actions`** — o que ele manda o servidor fazer. São verbos do
  `ClientMessage`, e passam pelas **mesmas permissões** que a janela atravessa:
  não há caminho paralelo, então não há semântica paralela para divergir.
- **`own`** — o quintal do MOD, e ele tem duas metades: chave→valor no banco do
  servidor, e **arquivos numa pasta que é dele**, em `mods/<autor>/<nome>/dados/`.
- **`world`** — o que não está no protocolo e não é do MOD: **rede de saída**,
  relógio e registro.

### O único freio da «liberdade total», e por que ele existe

O pedido do dono foi liberdade total desde a v1, e é o que está construído em
todo o resto. **O disco é a exceção, e ela é de uma linha:** um MOD lê e escreve
na pasta dele, e não na máquina.

O motivo não é gosto. É o que fica ao lado da pasta `mods/`, hoje:

```
identity.key   conhecidos   pins   seele.db   anexos   seele.log
```

Disco inteiro significaria **a chave privada de identidade**, os **pinos TOFU** e
o **banco com todas as conversas** — e aí o que cai não é uma regra de produto,
é o ADR 0004 e o 0017, que são o que garante que a pessoa é ela mesma. «As
regras do produto não alcançam MODs» é uma coisa; entregar a identidade de quem
hospeda junto é outra.

E rede e disco não são o mesmo grau, que é o que torna esta linha desenhável:
**rede deixa um MOD mandar para fora o que ele já enxerga; disco decide o que ele
enxerga.** Sozinha, a rede não alcança o `identity.key`.

**O que a pasta própria entrega mesmo assim:** volume de verdade — um MOD com
milhares de imagens não cabe num KV com teto —, importar e exportar, e uma pasta
que quem hospeda abre no Finder para ver o que o MOD guardou.

**A saída, para o caso que ela não cobre:** falar com outro programa da máquina
— OBS, um jogo, um bot que já roda ali — é o único uso que pede o disco inteiro,
e ele entra, se entrar, como **capacidade declarada no manifesto e mostrada em
separado na tela de aceite**, pelo caminho que o `reach` já foi desenhado para
fazer. Quem instala lê «este MOD lê o seu disco inteiro» como uma linha própria,
e não escondida dentro de «este MOD roda no servidor».

**O escopo é conferido pelo mesmo código que o `mod://` usa** —
`inner_path`, no `seele-proto`, que reconstrói o caminho por componentes e recusa `..` em vez
de resolver. Aquela função ganhou testes próprios depois de a prova por reversão
mostrar que ela não guardava nada; é por isso que ela é o alvo do mapeamento
aqui, e não uma segunda cópia da mesma ideia.

## O lado direito não é um caminho de código

O valor de cada nome é o **símbolo** que o `check-api` procura no repositório.
Ele é textual de propósito: um resolvedor de verdade exigiria o compilador, e o
que este guarda tem de pegar — um rename — muda o texto. O custo é que ele acha
o símbolo em qualquer lugar do repositório, e não exatamente onde o mapeamento
diz. É um guarda contra desaparecimento, e não contra mudança de lugar.
