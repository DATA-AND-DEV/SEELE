# A superfície que um MOD enxerga

Um arquivo por versão, e **nenhum deles é editado depois de publicado**.

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
  acha o mesmo nome no protocolo, sem intermediário.
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
