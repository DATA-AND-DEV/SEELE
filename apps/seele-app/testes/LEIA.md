# Vetores do indexador

Estes quatro arquivos **saíram do `ferramentas/gerar.py` do
`SEELE-MODS-INDEXER`**, assinados com a chave de MOD, e são copiados para cá
como vetor de teste. O mesmo recurso que `crates/seele-proto/tests/
vetores_de_hash.rs` já usa para o `content_hash`.

## Por que eles existem

Os dois lados desta cadeia moram em repositórios diferentes: o gerador e o
assinador num, o cliente que confere no outro. Cada um tem a sua suíte, e as
duas podem ficar verdes enquanto **discordam** — foi exatamente assim que o
cliente passou meses sem uma linha sobre o catálogo enquanto o indexador tinha
167 testes passando.

Um vetor assinado de verdade é o único jeito de uma suíte provar a outra sem
levantar rede. Se o formato do catálogo mudar de um lado, o teste que os lê
aqui fica vermelho.

## Quando trocá-los

Quando o `gerar.py` mudar o formato, ou quando a chave de MOD for trocada. A
troca é `cp` do `publicado/` do indexador, e o teste que os lê diz na hora se
este cliente ainda os entende.

**Este catálogo está vazio de propósito**: nenhum MOD foi avaliado ainda. Um
catálogo vazio e assinado é o estado correto de um indexador que acabou de
subir, e um cliente que o recuse recusaria o primeiro dia do serviço.
