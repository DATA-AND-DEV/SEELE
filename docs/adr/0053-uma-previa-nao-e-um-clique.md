# 0053 — Uma prévia não é um clique

Status: **aceito**
Data: 2026-09-21
Sobre o commit `5340290`, pela revisão da v15 (`docs/review-v15-2026-09-21.md`, R06).

> **Duas políticas para duas coisas.** Abrir um link é uma pessoa clicando;
> buscar uma prévia é esta janela indo sozinha por causa de um texto que outra
> pessoa escreveu. Tratá-las como uma foi o defeito.

## O que estava acontecendo

`pareceImagem`, na janela, aceitava **qualquer** host se o caminho terminasse em
extensão de imagem. `endereco_que_pode_sair`, no Rust, conferia esquema, tamanho
e caracteres — e nada sobre o destino. A regra de salto do cliente de prévia
repetia essa mesma conferência a cada redirecionamento.

Então:

- **desenhar uma mensagem era fazer uma requisição.** Quem publica um link
  aprende, só de ser buscado, o endereço de origem de quem está lendo a conversa
  e a hora em que leu. Um link posto na conversa só para colher endereços
  funcionava, e ninguém era consultado sobre isso;
- **o destino podia ser a máquina de quem lê.** A revisão verificou que
  `http://127.0.0.1:9999/private.png` era classificado como prévia automática, e
  que a cadeia nativa faria a busca. `http://192.168.0.1/` é o roteador da casa
  dessa pessoa; `http://169.254.169.254/latest/meta-data` é o serviço de
  metadados de uma nuvem;
- **conferir os bytes depois não evita a requisição.** `judge` é bom e não é
  sobre isto: ele decide se o que voltou é imagem, e o problema acontece antes de
  algo voltar.

Nada disso demonstra extração de conteúdo local nem execução de código, e este
documento não afirma isso. O que ele afirma é que **a requisição sozinha já é o
que não pode acontecer.**

## A decisão

**São duas políticas, e elas ficam em duas funções.**

`endereco_que_pode_sair` continua sendo a política de **abrir um link**: esquema
`http`/`https`, tamanho, nada de espaço ou controle. Ela não passa a recusar a
rede local, e é deliberado — `http://192.168.0.1` clicado por uma pessoa é o
roteador da casa dela, e recusá-lo seria o produto decidindo onde ela pode
navegar.

`endereco_de_previa` é a política de **esta janela buscar sozinha**. Ela é a de
cima, mais duas exigências:

1. **o destino não é reservado.** Loopback, as três faixas privadas, link-local,
   `0.0.0.0/8`, compartilhado entre operadoras, multicast, documentação, bancada
   de testes, e o equivalente em IPv6 — incluindo os endereços IPv4 mapeados,
   sem os quais `http://[::ffff:127.0.0.1]/` atravessa uma régua só de IPv4;
2. **o nome não é de um que por definição não é da internet pública**:
   `localhost`, `.localhost`, `.local`, `.internal`, `.home.arpa`, com ou sem o
   ponto final do nome absoluto.

**E a prévia automática depende de consentimento por domínio.** Por padrão nada é
buscado. No lugar da imagem, a conversa desenha o nome do domínio e um botão; o
clique libera aquele domínio, uma vez, e a liberação fica gravada nas
preferências. A configuração lista o que foi liberado e desfaz cada item — um
consentimento que não se enxerga é um consentimento que ninguém revoga, e aí «por
domínio, uma vez» vira «para sempre, sem lembrar de quem».

O domínio está no botão de propósito. A escolha é sobre entregar o endereço desta
máquina a **alguém**, e um «mostrar imagem» sem dizer a quem é um consentimento
que não se consegue dar de verdade.

## Redirecionamento e resolução

A regra de salto do cliente saiu. Quem segue os saltos agora é um laço em
`buscar_com_teto`, e **cada salto é uma ida completa**: valida o endereço,
resolve o nome, confere que **todos** os endereços resolvidos são públicos,
constrói um cliente fixado naqueles endereços, busca.

Duas coisas que só essa forma compra:

- **um `302` para a rede local não atravessa.** A política antiga conferia o
  salto pela régua de abrir link, que aceita `http://192.168.0.1`;
- **a troca de resolução não tem onde entrar.** Sem fixar o endereço, o nome é
  resolvido uma vez para conferir e outra para conectar, e nada obriga as duas
  respostas a serem a mesma. Com `resolve_to_addrs` não há segunda resolução.

«Todos os endereços públicos» e não «algum», porque a escolha não seria nossa: um
nome que resolve para um público e um privado deixaria a pilha do sistema
escolher, e metade das tentativas bateria na rede de quem lê.

## O que isto custa

- **um clique por domínio, na primeira vez.** É o preço, e ele é pago por quem
  recebe o benefício, o que é o oposto de antes;
- **um serviço de imagem hospedado na rede local não é previsualizado.** Quem
  tem um alcança pelo botão de abrir link, que continua aceitando. Não conheço
  esse caso em campo; se ele aparecer, a saída é uma exceção nomeada por quem
  hospeda, e não relaxar a régua;
- **a lista de domínios de GIF não dispensa consentimento.** Ela diz quais
  páginas são resolvidas até a mídia que declaram, e não que buscá-las seja de
  graça. Um `tenor.com` também passa pelo botão na primeira vez.

## O que continua de pé

- A CSP não se mexe: `img-src 'self' data: mod:`. A busca continua sendo do Rust,
  e o que chega à tela continua sendo um `data:` conferido contra os bytes.
- Nenhum cookie vai junto, nenhum `Referer` diz de onde veio.
- O teto de 4 MiB continua aplicado **enquanto** os bytes chegam, e não depois.
- `pareceImagem` continua sendo o segundo filtro: com o consentimento dado, é ele
  que decide que uma página qualquer não é uma imagem.

## Provas

- `o_que_sai_desta_janela_por_um_link::uma_previa_nao_alcanca_a_maquina_nem_a_rede_de_quem_le`
  — inclui a reprodução literal da revisão, e prova junto que a política de abrir
  link **continua** aceitando os mesmos endereços: se as duas voltarem a ser uma,
  o teste reprova.
- `uma_previa_alcanca_a_internet_publica` — a outra metade, sem a qual uma porta
  fechada para todo mundo passaria no teste de cima.
- `cada_salto_da_previa_e_validado_resolvido_e_fixado` e
  `a_previa_pergunta_antes_de_buscar` — a ordem importa: um consentimento
  conferido depois da busca é um consentimento pedido depois de a requisição já
  ter aparecido no servidor de outra pessoa.
- `frontend::nenhuma_previa_sai_sem_consentimento` — as três metades da janela:
  quem desenha não busca, o botão nomeia o domínio, e a configuração desfaz.
