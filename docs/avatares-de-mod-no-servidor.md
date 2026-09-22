# Avatar de servidor em todas as superfícies

O ponto de contribuição `pessoa.avatar` aplica uma imagem por ID de pessoa aos
avatares das mensagens, dos cartões de chamada e da barra inferior do operador.
O cartão da lista de pessoas continua sendo apresentado por `pessoa.cartao`.

```js
const { handle } = await SeeleUI.contribuicoes.registrar({
  ponto: 'pessoa.avatar',
  modo: 'substituir',
  alvo: String(pessoaId),
  prioridade: 10,
  conteudo: {
    doServidor: {
      canal,
      pedido: { op: 'asset', person: String(pessoaId), slot: 'avatar', path },
      campo: 'image',
    },
  },
});
```

O MOD declara a origem no próprio servidor. O produto busca, valida e compartilha
os bytes entre as aparições da pessoa, mantendo os controles e os IDs nativos.
Para imagens por volume, o pedido inclui `transporte: 'volume'`, como no retrato
das regiões. O cache de avatares tem teto de 20 MiB por instância.

O alvo e `conteudo.doServidor` são obrigatórios. Este ponto aceita somente
`substituir`; não monta árvores de região nem aceita URLs externas.
A preferência de avatar, quando definida, tem precedência; em automático ela
segue a preferência de `pessoa.cartao`. Sem preferência, vale a prioridade do
registro. A escolha nativa, uma imagem recusada ou a ausência de avatar usam o
retrato nativo; sem ele, aparecem as iniciais.

Ao trocar a origem, revogue o handle anterior e registre o novo. Ao remover a
imagem, revogue o handle. A revogação e o encerramento da instância descartam o
cache e impedem cargas pendentes de alterar a apresentação. O pacote PERFIS
publica essas referências junto com seus cartões e preserva os handles enquanto
a origem permanecer igual.

Esta extensão requer o SEELE com suporte a `pessoa.avatar` e o PERFIS atualizado.
O PERFIS mantém seus cartões em versões anteriores que recusam o ponto.
Os descritores de API já publicados permanecem congelados.

Validação: `node apps/seele-app/bancada/avatares-do-mod.cjs` executa o HTML e CSS
do produto em Chromium com ponte nativa simulada. Cobre as três superfícies,
carga única, preferência nativa, substituição, remoção e resposta tardia.
Os testes do pacote PERFIS cobrem publicação e revogação pelo cliente real.
