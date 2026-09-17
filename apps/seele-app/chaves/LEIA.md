# A chave pública do catálogo de MODs

`mods.pub` é uma **cópia byte a byte** de `chaves/mods.pub` do repositório
`SEELE-MODS-INDEXER`. Ela vive aqui porque o cliente a compila para dentro do
binário: o catálogo é conferido contra a chave **embutida**, e não contra uma
que venha junto do arquivo baixado — uma chave que chega pela mesma porta que o
conteúdo não prova nada sobre ele.

## Por que é a segunda chave, e não a do atualizador

ADR 0045 (e o ADR 0026 que ele cita): *«uma chave que atesta duas coisas deixa
as duas se passarem uma pela outra, e a do 0026 autoriza instalar programa»*. A
do atualizador diz «este é o SEELE»; esta diz «este MOD passou pela avaliação».
Fundi-las deixaria um catálogo de MOD assinado poder se passar por uma
atualização do produto.

## Trocar esta chave

Não é uma edição de arquivo: é uma release. Todo cliente publicado tem a chave
antiga compilada dentro, e um catálogo assinado com a nova é um catálogo que
eles recusam. A ordem é publicar o cliente que conhece as duas, esperar, e só
então trocar a assinatura do catálogo.
