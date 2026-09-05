# A superfície que um MOD enxerga

Um arquivo por versão, e **nenhum deles é editado depois de publicado**.

## Por que congelado

O [ADR 0044](../docs/adr/0044-mods-o-produto-base-tem-regras-e-um-mod-nao.md)
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
- **`own`** e **`world`** — o quintal de dados do MOD, e o que não está no
  protocolo: rede, relógio, registro e **disco**.

`world` é onde a «liberdade total» do ADR 0044 mora, e é a parte que a tela de
aceite tem de dizer em voz alta, em letra grande e sem eufemismo:

> **Um MOD alcança a rede e o sistema de arquivos da máquina de quem hospeda.**

Não há caixa de areia, por decisão do dono — *«o que construímos até agora tem
regras, mas essas regras não se aplicam aos MODs, e essa é a graça»*. A defesa é
o repositório público obrigatório e a revisão de código de cada versão
publicada, e ela não é técnica: é humana, e é a única. Está escrita como tal em
«O que fica sem saída» do ADR.

O `lerArquivo` e o `escreverArquivo` chegaram depois dos outros três, e a razão
está registrada: a primeira versão desta página tinha rede e não tinha disco, o
que era menos do que «liberdade total» quer dizer. Um MOD que abre túnel já
alcança tudo o que o disco alcançaria pela rede; a assimetria era acidente, não
desenho.

## O lado direito não é um caminho de código

O valor de cada nome é o **símbolo** que o `check-api` procura no repositório.
Ele é textual de propósito: um resolvedor de verdade exigiria o compilador, e o
que este guarda tem de pegar — um rename — muda o texto. O custo é que ele acha
o símbolo em qualquer lugar do repositório, e não exatamente onde o mapeamento
diz. É um guarda contra desaparecimento, e não contra mudança de lugar.
