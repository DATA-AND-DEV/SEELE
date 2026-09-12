# Artefatos de teste do `seele-lancador`

**Nada aqui é de produção, e nenhuma versão real do SEELE é encenada nestes
arquivos.** A chave foi gerada para este teste e a única coisa que ela assina é
a lista de revogação inventada que está nesta pasta, cujo alvo é a versão
`0.0.0-artefato-de-teste`, que não existe e nunca vai existir.

| arquivo | o que é |
|---|---|
| `chave-publica-de-teste.base64` | a chave pública, no formato em que ela viveria no `tauri.conf.json`: base64 do arquivo `.pub` inteiro |
| `lista-de-revogacao-de-teste.json` | os bytes exatos que foram assinados |
| `assinatura-da-lista-de-teste.base64` | o `.sig`, em base64, como ele viaja no manifesto |

A **chave privada não está aqui e foi descartada.** Ela não tem uso: estes
arquivos existem para serem conferidos, e conferir não precisa dela. Quem
precisar de outra fixture gera um par novo com os comandos abaixo, em vez de
reusar este.

## Por que estes arquivos existem, se o teste já sabe assinar

`crate::assinatura::fixtures` assina dentro do teste, com `ed25519-dalek` e
`blake2`, e é o que permite provar que uma assinatura errada é recusada.
Mas ele prova isso contra a **nossa** ideia do formato do minisign. Se essa
ideia estivesse errada nos dois lados — no que assina e no que confere — os
testes passariam e o produto recusaria todo pacote de verdade. Isto é o que o
`CLAUDE.md` chama de *«existir não é funcionar»*.

Estes três arquivos saíram do `minisign` de verdade, que é o mesmo formato que
o `cargo tauri signer` produz e que o `tauri-plugin-updater` confere. São a
única prova de que o formato lido aqui é o formato que circula.

## Como foram gerados

```sh
minisign -G -f -p chave.pub -s chave.key -W
printf '%s' '{"schema":1,"issued_at":…}' > lista-de-revogacao-de-teste.json
minisign -S -s chave.key -m lista-de-revogacao-de-teste.json -x lista.sig \
  -c "ASSINATURA DE TESTE DO SEELE-LANCADOR" \
  -t "artefato de teste do seele-lancador"
minisign -V -p chave.pub -m lista-de-revogacao-de-teste.json -x lista.sig

base64 < chave.pub  > chave-publica-de-teste.base64
base64 < lista.sig  > assinatura-da-lista-de-teste.base64
```
