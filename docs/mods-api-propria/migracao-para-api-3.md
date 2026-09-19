# A sequência de migração para a API 3

Desenvolvimento e publicação são entregas separadas. O desenvolvimento está
pronto; esta página é a ordem em que ele chega às pessoas **sem quebrar quem já
está no ar**.

A regra que governa tudo aqui é uma linha de `read_manifest`:

```rust
if manifest.api != MOD_API_VERSION { recusa }
```

**Igualdade, e não «até».** A API 3 tirou coisa — `document`, o global do
Tauri, a página inteira —, e um MOD de API 2 não é um MOD que pede menos: é um
MOD que pede o que não existe mais. Carregá-lo o faria falhar na primeira linha,
e quem o instalou leria «não carregou» sem saber que o motivo era a versão.

A consequência prática: **um cliente só roda MODs da própria API**. Não há
período em que os dois convivem no mesmo aplicativo, e a ordem abaixo existe
por causa disso.

## O que já está feito

| Peça | Estado |
| --- | --- |
| `MOD_API_VERSION` no produto | **3** |
| `VERSAO_DA_API` no indexador | **3** |
| `api/v3.json` — o contrato | escrito, conferido por `cargo xtask check-api` |
| Vetor `api-do-indexador.json` | no repositório, e o guarda compara com ele |
| Executor | QuickJS, sem variável de ambiente e sem alternativa |
| Os três MODs em 2.0.0, API 3 | escritos e com suíte verde (14, 31 e 41 provas) |
| A matriz dos três MODs | sem pendência: os seis grupos fechados |
| `SeeleUI.marcas` — a lista de pessoas | no prelúdio, no contrato e no vetor de referência |
| Guia e exemplo | revisados e regerados |
| Catálogo publicado | **ainda diz `api_oferecida: 2`** |

O catálogo publicado ficar atrás é o estado normal entre implementar uma versão
e publicá-la. O guarda que compara os dois já sabe disso: ele exige igualdade
com o **código** do indexador e só exige `<=` do catálogo publicado — um
catálogo à frente seria um cliente sendo oferecido MODs que ele não roda, e é
esse o caso que ele pega.

### O que fechou por último

Os seis grupos que a matriz ainda marcava pendentes foram fechados, e nenhum
exigiu API nova além de uma:

- **MESA**, quatro grupos de tela: magias e espaços, ações com fórmula e usos,
  edição e publicação de verbete, e ajuste de cena. Cada um já tinha operação no
  servidor e forma que o atendia; o que faltava era desenhar.
- **ESTILO**, arredondamento e brilho: fechados como **recusa nomeada**, porque
  `docs/marca.md` proíbe raio e sombra com a palavra «nunca». A API devolve a
  razão e a citação em vez de dizer que não conhece o nome.
- **PERFIS**, a lista de pessoas: fechado com `SeeleUI.marcas`, a única
  superfície nova desta rodada. Ela não devolve a janela ao MOD — ele entrega
  texto e cor por pessoa, e quem desenha é o produto.

`MOD_API_VERSION` continua **3**: `marcas` entrou na mesma versão que ainda não
foi publicada, e por isso não há salto de versão a fazer. Se ela já estivesse no
ar, esta linha diria 4.

## A ordem, e por que é esta

### 1. Publicar o aplicativo com a API 3

**Antes do catálogo.** Um cliente de API 3 entende um catálogo que diz 2: ele
simplesmente não encontra MOD nenhum que possa instalar. O contrário não vale —
um cliente de API 2 diante de um catálogo de API 3 vê pacotes que ele recusa, e
a recusa chega como «não carregou».

Quem está em API 2 continua com os MODs de API 2 que já tem instalados,
funcionando, até atualizar.

### 2. Regerar e assinar o catálogo com `api_oferecida: 3`

`ferramentas/gerar.py` no indexador, com a chave de MOD. **É o passo que precisa
da chave, e é o único.**

Os pacotes históricos de API 2 permanecem no catálogo, como histórico. Eles não
são substituídos: um pacote publicado é um pacote publicado, e apagá-lo quebra
a conferência de quem o tem instalado.

### 3. Publicar os três MODs em 2.0.0

MESA, PERFIS e ESTILO, com `api: 3`. Eles já estão escritos e testados; o que
falta é a assinatura e a entrada no catálogo.

**O PERFIS 2.0.0 depende do passo 1.** Ele chama `SeeleUI.marcas`, que só existe
no aplicativo com a API 3 — publicá-lo contra um aplicativo sem essa função o
faria falhar na primeira volta do relógio. A ordem já o cobre: o aplicativo sai
antes. O que este parágrafo acrescenta é que agora há uma razão concreta, e não
só a regra geral.

### 4. Copiar o catálogo novo para os vetores deste repositório

`cp` do `publicado/` do indexador para `apps/seele-app/testes/`, os dois
arquivos e as duas assinaturas. O `LEIA.md` de lá descreve exatamente isso.

Depois desse `cp`, o guarda passa a comparar um catálogo de API 3 com um build
de API 3, e as duas metades voltam a dizer a mesma coisa sobre o presente.

## O que quebra se a ordem for outra

**Catálogo antes do aplicativo:** todo cliente de API 2 no ar passa a ver três
pacotes que ele recusa. A tela diz «não carregou» por MOD, e a causa — a versão
— não aparece em lugar nenhum que quem usa leia.

**MODs antes do catálogo:** o catálogo é o que anuncia o que existe. Publicar
pacote sem anunciá-lo é publicar nada.

**Constante antes de tudo, sem publicar:** foi o que quase aconteceu aqui. Um
build de API 3 recusa **todo MOD instalado hoje**, porque todos eles declaram
API 2. Quem atualizasse perderia os três de uma vez, sem aviso.

## O que não está coberto por esta sequência

**Servidores já instalados.** A metade de servidor de um MOD roda na máquina de
quem hospeda, e ela não tem versão de API própria — o manifesto é um só. Um
servidor com MESA 1.2.1 instalado continua servindo MESA 1.2.1 para clientes de
API 2; quando quem hospeda atualizar o pacote para 2.0.0, ele passa a servir a
versão nova. Os dois nunca convivem no mesmo servidor, porque o pacote é um só.

**A janela de desencontro.** Entre o passo 1 e o passo 3 existe um intervalo em
que alguém com o aplicativo novo não acha MOD nenhum para instalar. Ele é curto
se os passos forem seguidos juntos, e é preferível ao contrário: não achar nada
é uma tela vazia, e achar o que não roda é uma tela com três erros.
