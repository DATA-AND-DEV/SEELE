# Notas da versão — v0.11.0

> **Esta página é o texto para o corpo do release**, no formato das publicadas.
> Ela fica no repositório porque a v0.11.0 ainda **não foi publicada**: a tag e
> a publicação são passo manual de quem opera, e estão descritos no fim.

_As mudanças de `v0.10.5-1` até `v0.11.0`. São **188 commits em 13 dias**._

---

## ⚠️ Esta versão não fala com as anteriores

**Leia antes de atualizar uma máquina só.**

A `v0.10.5-1` publicada fala **protocolo 3**. A v0.11.0 fala **protocolo 6**, e
a janela de compatibilidade alcança uma versão para trás — 6 e 5. A v3 está
fora dela.

Conferido lendo o commit que gerou a release publicada (`12a6401`), e não
suposto: `PROTOCOL_VERSION = 3`, `COMPATIBILITY_WINDOW = 1`.

**Os dois lados se recusam, e é recusa mútua e simétrica:**

| quem tenta | o que acontece |
|---|---|
| cliente v0.10.5-1 → servidor v0.11.0 | recusado: `PeerTooOld` vira `Incompatible` |
| cliente v0.11.0 → servidor v0.10.5-1 | recusado: `PeerTooNew` vira `Incompatible` |

**O que a pessoa vê**, nos dois casos, é a mesma frase:

> VERSÃO INCOMPATÍVEL COM ESTE SERVIDOR.
> Um dos dois lados está desatualizado — atualize o SEELE nas duas máquinas e
> tente de novo.

A frase diz «um dos dois» e não «o seu» de propósito: o motivo chega à tela
como o mesmo `Incompatible` nas duas direções, e mandar *você* atualizar
erraria metade das vezes — justamente com quem já atualizou.

**Consequência prática: atualize todas as máquinas do grupo juntas.** Não há
caminho de convivência, e não é descuido — o `postcard` indexa variante por
posição e não é autodescritivo, então uma mensagem desconhecida não é ignorada:
ela desloca a leitura do fluxo de controle para sempre. Falar duas versões do
protocolo significaria manter dois conjuntos de tipos vivos.

---

## O que mudou

### A tela compartilhada pode vir de outra pessoa, e não só do servidor

O caminho entre pares. Quem assiste passa a poder receber a tela direto de quem
a transmite, com consentimento dos dois lados, e o servidor reassume sozinho
quando o par falha ou sai. Em sala com muita gente, é a subida de quem hospeda
que deixa de ser o teto.

**O consentimento fica em CONFIGURAÇÕES · MALHA, e são dois**, porque são duas
perguntas: emprestar a sua subida é *quanto da sua internet você gasta pelos
outros*; aceitar ser servida é *aceito que o meu endereço seja entregue a quem
me servir*. Quem aceita uma não aceita a outra por tabela. Sem escolher nada,
você não participa — a tela vem do servidor, como sempre veio.

**A frase acima quase saiu mentindo.** Até 17/09 o protocolo, o núcleo e o
servidor traziam esta metade inteira e **não havia onde consentir**: nenhum
comando do aplicativo chamava o empréstimo, e em produção ele era sempre falso.
Esta nota prometia «com consentimento dos dois lados» a quem não tinha onde dar
consentimento nenhum.

### MODs saem do papel

O servidor passa a **rodar** os MODs habilitados, e não só a conhecê-los. O
anúncio e o aceite atravessam o protocolo: quem entra vê a lista — quem é o
MOD, de onde ele veio e o que ele alcança — antes de qualquer byte ser baixado,
e um MOD obrigatório recusado barra a entrada em vez de deixar a sessão meio
montada.

E entra a **ponte de pedidos**: um MOD pode perguntar ao servidor e receber uma
resposta que vai **só para quem perguntou**, em vez de sair pelo barramento que
todo mundo lê. A identidade de quem pergunta é escrita pelo servidor a partir da
sessão autenticada, nunca pelo pedido — um cliente que a forje não muda nada.

E as duas telas, sem as quais nada disso era usável. **Ao entrar** num servidor
que exige MODs, uma tela mostra a lista antes de qualquer byte: quem é cada MOD,
que versão, que hash, de que repositório, o que ele alcança, e quais rodam na
máquina de quem hospeda. Aceitar vale para aquela combinação; trocou um MOD, a
pergunta volta — e a tela avisa quando o servidor mudou a lista desde o seu
último sim. **Em CONFIGURAÇÕES · MODS**, quem hospeda instala um MOD de uma
pasta, liga e desliga; e qualquer pessoa vê a quem já disse sim e pode desfazer.

Até esta versão nada disso tinha tela: um servidor com MOD habilitado
simplesmente **não tinha como ser entrado** pelo aplicativo.

### O indexador de MODs está no ar

`mods.seele.app.br` abriu nesta rodada, e com ele o catálogo deixa de ser
promessa: em **CONFIGURAÇÕES · MODS**, um botão busca o catálogo, lista o que
há com o que cada MOD alcança, e instala sem sair do aplicativo — além de
apontar para uma pasta, que continua valendo. Não há filtro de texto, e nada é
consultado ao abrir a seção: a busca sai de um botão, para que abrir
CONFIGURAÇÕES não conte a ninguém que você a abriu.

O catálogo **inteiro** é assinado uma vez, na máquina de quem tem a chave, e o
aplicativo o confere contra a chave pública que veio compilada dentro dele. Não
há assinatura por autor: o que prova os bytes de cada versão é o hash do
conteúdo, e o que fixa o que foi avaliado é o commit. Um push do autor depois da
aprovação não muda o que ninguém baixa.

O primeiro MOD avaliado é o **MESA** 1.2.0 — uma mesa de RPG por canal, com
fichas, tabuleiro, compêndio e iniciativa, enquanto a voz continua na sala do
SEELE. Ele saiu como `verificado`, com as duas notas que a tela mostra antes do
botão de instalar: metade dele roda na máquina de quem hospeda, e ele grava
coisas em disco.

**E a avaliação é um filtro, não uma prova.** Ela pega o óbvio — código que
baixa e executa texto de fora, exfiltração escancarada, travessia de caminho — e
não pega o caminho sutil na décima função de um arquivo limpo. «Verificado»
quer dizer que passou no filtro.

### E isto custa uma dependência que o SEELE não tinha

Um produto que se vende como auto-hospedado passou a ter uma peça numa CDN: o
catálogo é servido pelo Cloudflare Pages, e quem busca um MOD aparece nos
registros que a Cloudflare guarda para si. Isso é dito aqui em voz alta em vez
de ficar para alguém descobrir.

O que **não** foi ligado, e a ausência é a decisão: nada de análise de tráfego,
nada de Logpush, nada de etiqueta de terceiro. O argumento do ADR 0045 para um
catálogo estático é que «com API o indexador aprende cada termo que alguém
digitou; com catálogo, aprende que alguém buscou o catálogo» — ligar análise
desfaria isso por fora, sem tocar numa linha de código.

E nada do servidor depende dela: um SEELE hospedado por você funciona inteiro
sem jamais falar com `mods.seele.app.br`. O que se perde sem a CDN é procurar
MODs novos, não usar os que já estão instalados.

### A tela compartilhada começa boa, e não borrada

A resolução sai de um teto medido, e a medida partia de uma suposição de
2 Mbps enquanto ninguém tinha medido nada — 60% disso é 1,2 Mbps, e 720p pede
2,76. Toda transmissão começava em 540p e subia conforme a sonda descobria o
cano: medido em campo, 1,20 → 1,58 Mbps em **vinte e cinco segundos** de
imagem ruim.

Agora a máquina lembra a própria subida entre sessões, e a sonda parte dela. A
medida de cada servidor visitado continua tendo precedência, porque o caminho
até cada um é diferente; a da máquina cobre os dois casos que faltavam —
hospedar aqui, e a primeira visita a um servidor novo.

**Quem hospeda era o mais prejudicado**, e por um detalhe: a semente existia e
ficava dentro do filtro que exclui `127.0.0.1` da lista de servidores
visitados. Com o loopback por baixo e a subida inteira disponível, o primeiro
segundo continuava sendo 540p.

### Trocar de fone ou de microfone vale na hora

Era preciso reiniciar o aplicativo. O produto ignorava o aviso que o sistema já
mandava — ele contava «mais um erro» e jogava fora o tipo dele —, e o laço
seguia falando com um aparelho que já não era de ninguém.

Agora a troca feita **fora** do SEELE vale: a bandeja do Windows, as
Configurações do Mac, o fone puxado da tomada. A interface é avisada enquanto a
troca acontece, em vez de um aviso de falha local que apagava sozinho.

O som da **tela compartilhada** segue junto: ele abria uma vez e ficava preso ao
aparelho de quando a transmissão começou.

### Voltar depois de cair não apaga você

Uma conexão que morria depois de a pessoa já ter voltado por outra apagava o
estado da nova: a pessoa sumia do roster de todo mundo continuando a se ver
dentro da sala. Toda a saída passa a ser guardada por identificador de sessão.

E o fantasma do outro lado saiu junto: reconectar deixava uma cópia velha da
contabilidade de pé por cima da fotografia nova.

### Expulsar passa a manter alguém fora

`:expulsar` encerrava a sessão da pessoa e, em milissegundos, ela estava de
volta na mesma sala — o verbo interrompia a conexão e não removia ninguém de
nada por mais de um instante. Eram duas metades, e cada uma bastava sozinha:

- **`EnterVoiceRoom` não conferia `Permission::EnterVoiceRoom`.** A permissão
  existia no protocolo e não era lida em lugar nenhum. Agora é.
- **A bateria interna do cliente reconectava e restaurava a sala**, sem
  distinguir «a rede caiu» de «um operador me tirou daqui». Agora `Kicked`,
  `Banned` e `ModsMudaram` são tratados como fim de sessão, e não como queda.

**O que continua aberto, e está registrado na pendência 45:** nada disto foi
provado em campo, com duas máquinas e alguém expulsando de verdade. E o
servidor continua «confirmando a entrada por silêncio» em vez de responder a
ela, que é a terceira parte do mesmo achado.

### Entrar numa sala com senha funciona, e o teto e a permissão passam a valer

A senha de sala de voz era declarada no protocolo, anunciada ao cliente e
**nunca conferida** — uma fechadura que se anuncia trancada e não está.

Junto dela, dois números que o produto anunciava e não cumpria: o **limite de
pessoas** que quem hospeda escreve passa a barrar de verdade, com o motivo
«sala cheia»; e a **permissão de entrar** passa a ser conferida pelo servidor,
e não só escondida na tela.

### Versões lado a lado

O app vira launcher, e as três metades estão de pé.

**Guardar uma versão ao lado** — em CONFIGURAÇÕES · VERSÕES, o botão baixa do
catálogo a mais nova e a guarda **sem substituir** a que está aberta. Cada
versão tem o diretório de dados dela.

**Hospedar com outra versão** — na tela de entrada, quando há mais de uma
guardada.

**E entrar num servidor abre a versão daquele servidor**, que é a frase da
decisão. A versão que hospeda viaja no próprio `seele://`, e é lida **antes** de
conectar: um servidor de uma versão anterior pode falar um protocolo que este
cliente já não alcança, e seria recusado com «versão incompatível» sem chegar a
dizer uma palavra sobre si. O link chega antes disso.

**O que não vale no Windows:** o que o catálogo publica ali é um instalador
`.exe`, que instala por cima da instalação única da máquina. Guardar versões ao
lado precisa de um pacote que se abra numa pasta, e ele ainda não existe no
catálogo para Windows.

### Um link com nome, em vez de um número que muda

Quem tem um nome apontado para a máquina pode pô-lo no link, e ele passa a
valer enquanto o nome valer. O endereço numérico vai junto, atrás: quem não
resolver o nome entra por ele do mesmo jeito.

A receita de DNS está em `docs/alcance-pela-internet.md`, com a ressalva que
importa — quem já entrou pelo endereço numérico vai ser perguntado de novo
sobre a impressão digital ao entrar pelo nome.

### UPnP: o link guardado volta a servir

Quando a porta externa 8383 estava ocupada por um mapeamento antigo do próprio
SEELE, o servidor desistia dela e aceitava qualquer porta — e o link guardado
por quem hospeda deixava de funcionar. Agora a porta canônica é tentada de novo
antes do recuo, e **quando o recuo acontece quem hospeda é avisado** de que o
link gerado pode não servir depois de reiniciar.

### No Windows, um instalador só

O setup do NSIS saiu. Uma página de release com `.exe` e `.msi` lado a lado
obriga quem chegou a escolher antes de entender a diferença — e a primeira
escolha de quem não sabe é fechar a aba. O que fica é o instalador que não pede
administrador.

### Uma falha de TLS fechada antes de sair

O `rustls` que o SEELE usa aceitava mensagens de aperto de mão do TLS 1.3
atravessando a fronteira entre níveis de cifra — o que a RFC 8446 proíbe na
seção 5.1, e o que permitiria a alguém no meio do caminho injetar mensagem em
claro num aperto de mão que devia estar protegido. É a RUSTSEC-2026-0285, e ela
alcançava tudo: o `quinn`, que é todo o transporte QUIC entre cliente e
servidor, e o `reqwest`, que busca atualização e catálogo de MODs. Em todos os
sistemas.

A dependência subiu para 0.23.45, que é a versão que corrige.

Ela quase não foi vista. A conferência de dependências reprovava com o erro na
**primeira** linha e dez avisos depois dele — dez ignorados que apontavam para
avisos de GTK3 que o RustSec já havia retirado, e que continuavam ali fazendo
volume. Os dez saíram. O que sobra da conferência é o que importa.

### A suíte de testes volta a ser evidência

A bateria de conformidade reprovava um teste por rodada, sempre um diferente,
sempre por prazo. Não era defeito de produto: cada teste levanta um servidor
QUIC de verdade, e os apertos de mão concorrentes estouravam o prazo do
transporte. A conformidade passa a ser serializada dentro do próprio crate,
sem serializar o resto.

E o job de teste da integração contínua, que nunca rodou um doctest sequer,
passou a rodar; as três conferências de regra do repositório, que existiam e
ninguém executava, passaram a rodar a cada push.

E a bateria passou a reprovar no Windows — em dois testes, e em nenhum outro.
Não era o produto: os vetores assinados chegavam convertidos. O Git do Windows
troca LF por CRLF na cópia de trabalho por padrão, e uma assinatura vale sobre
**bytes**; o catálogo do indexador chegava lá com 226 bytes de `\r` a mais do
que o arquivo que foi assinado, e a conferência recusava — como tinha de
recusar. A conversão alcançava também a chave pública que este build confere —
que, medido depois, continua sendo lida mesmo convertida: o analisador tolera o
`\r`. Ninguém recebeu binário quebrado por isso.

Um `.gitattributes` tira do conversor os oito arquivos cujos bytes são o
contrato. E um guarda reprova **aqui**, em qualquer sistema, se o próximo vetor
assinado entrar sem ser declarado — porque o conserto é invisível, e quem o
esquecer vai ver a própria máquina passar.

---

## Todos os commits desta versão

São 188, do mais recente ao mais antigo. O resumo curado está acima; esta lista
é para quem precisa achar o commit que mexeu numa coisa específica.

> Gerada com `git log --oneline v0.10.5-1..v0.11.0 --no-merges` na hora de
> publicar. Ela não está copiada aqui de propósito: uma lista copiada à mão é
> uma lista que fica velha entre o texto e a tag.

---

## O que baixar

**Um arquivo por sistema.** Ele traz as duas metades do SEELE: o cliente
gráfico e a ferramenta de terminal.

| sistema | baixe |
|---|---|
| **Windows** | `SEELE_<versão>_x64-setup.exe` |
| **macOS** | `SEELE_<versão>_universal.dmg` — Intel e Apple Silicon |
| **Linux** | `SEELE_<versão>_amd64.deb` — Debian, Ubuntu e derivados |

Dentro de cada um vão **`SEELE`**, o cliente gráfico, que tem um botão
**HOSPEDAR AQUI** e com o qual você nunca precisa abrir um terminal; e
**`seeled`**, o servidor, para quem quer um servidor no ar o tempo todo — só uma
das máquinas precisa dele.

Os outros arquivos desta página não são para instalar. O `SHA256SUMS` serve para
conferir que o download chegou inteiro; os `.sig` e o `latest.json` são como o
próprio SEELE se atualiza sozinho.

---

## O que esta versão **não** resolve

Dito aqui porque a página de release é onde se procura, e porque metade destes
tem relato de campo em aberto.

- **Compartilhar tela entre duas máquinas Windows** continua sem funcionar
  (pendência 33). Os quatro suspeitos foram eliminados por medida; o que sobra
  é o caminho de captura de verdade, que exige sessão gráfica para investigar.
- **O job `windows-2022` da integração contínua nunca rodou.** O workflow existe
  escrito e conferido, e ninguém o viu executar (pendência 38). Adiado por
  decisão explícita nesta rodada.
- **O caminho de Windows do som da tela não é compilado no Mac de quem fechou
  esta versão** (pendência 46). A parte que decide foi tirada de dentro do `cfg`
  e é testada; o que sobra sem compilação é uma chamada e um `match`.
- **A tela de aceite não mostra selo de «oficial» ou «verificado»**, e mostra
  hash e repositório. O indexador subiu no meio desta rodada e o catálogo é
  conferido por assinatura (abaixo), mas o aceite fala dos MODs que **um
  servidor declara**, e um servidor pode declarar um MOD que não está no
  catálogo nenhum. Cruzar as duas listas é trabalho que o código ainda não faz,
  e um selo que o código não sustenta é justamente aquilo em que alguém se
  apoiaria para dizer sim sem ler o resto.
- **O caminho entre pares nunca rodou sobre rede de verdade.** O teste de
  conformidade sobe servidor e clientes reais, e roda tudo **em processo**: a
  discagem entre duas máquinas numa rede local, com NAT e roteador no meio,
  não foi exercitada uma vez. O que está provado é a lógica; o que falta é o
  fio.
- **Versões lado a lado não valem no Windows**, pelo motivo acima.
- **Descer de versão não leva as conversas junto.** Cada versão tem o próprio
  diretório de dados, de propósito, e o produto diz isso na tela.

---

## Como esta versão foi montada, e o que ficou por provar

**Nada foi publicado.** Nenhum push, nenhuma tag, nenhum binário enviado. A tag
e a publicação são o passo manual descrito abaixo.

### Os números desta volta

Medidos do commit que gerou a release publicada (`12a6401`) até `29dee8c`, e
não estimados. O corte é ali de propósito: o commit que escreve estas linhas
mudaria todos eles, e um número que conta a si mesmo não tem onde parar.

| | v0.10.5-1 | v0.11.0 |
|---|---|---|
| protocolo | 3 | **6** |
| API de MODs | não existia | **2** |
| funções de teste | 1 609 | **2 161** |
| crates | — | **+1** (`seele-lancador`) |
| comandos da ponte | — | **+24**, −2 |

397 arquivos mudaram: +112 821 / −1 469 linhas. As áreas que mais cresceram
foram documentação, servidor, conformidade e núcleo, nesta ordem.

**Quatro decisões novas ficaram registradas como ADR**, e cada uma governa uma
das entregas acima:

- **0044** — o portão divide a subida medida.
- **0045** — MODs: o produto base tem regras, e um MOD não.
- **0046** — toda versão continua de pé.
- **0047** — o link que volta a funcionar amanhã.

### O que foi exercitado de verdade, nesta máquina

- `./empacotar/macos.sh 0.11.0` — **saída 0**. Saíram
  `SEELE_0.11.0_aarch64.dmg` e `seele-cli-0.11.0-macos.tar.gz`.
- O `seeled` empacotado responde `seeled 0.11.0` a `--versao`, e o binário do
  app contém a cadeia `0.11.0` e **não** contém `connection/local` — que é a
  prova de que o carimbo de versão chegou às duas metades, e não só ao `seeled`.
  Era um defeito desta rodada: `SEELE_VERSAO` não alcançava o build do app.
- O `tauri.conf.json` voltou a `0.0.0` sozinho depois do empacotamento, como o
  `trap` do script promete. A versão é do artefato, nunca do repositório.
- **O checkout do Windows foi simulado aqui**, com `git clone -c
  core.autocrlf=true`. Antes do `.gitattributes`: 47 reprovados em 83 suítes.
  Depois: a árvore sai byte a byte igual à do Mac — 727 arquivos, zero
  diferenças — e a bateria fecha em **2 152 passando, nenhuma falha**.

### O que esta máquina **não** pôde provar

- **O `.dmg` é só `aarch64`.** Um Mac Intel não o abre. O universal sai do
  workflow, que junta os dois com `lipo`.
- **Windows e Linux não foram empacotados.** Esta é uma máquina só.
- **Nada foi assinado nem notarizado**: a assinatura é ad-hoc (`-`), que faz a
  permissão de microfone grudar e **não** vale para o Gatekeeper. Sem
  `TAURI_SIGNING_PRIVATE_KEY` não saiu pacote de atualização — este `.dmg`
  serve para instalar à mão e não serve como origem de atualização.
- **O job `windows-2022` continua sem nunca ter rodado** (pendência 38). A
  simulação acima cobre o fim de linha, que é o que quebrou desta vez — e só
  isso. Nenhum código de caminho de Windows foi executado num Windows.

### O passo manual, para quem for publicar

1. Confira que a árvore está limpa e que a validação está verde — os comandos e
   os números estão no fim desta página.
2. Crie a tag: `git tag -a v0.11.0 -m "v0.11.0"`.
3. Empurre-a: `git push origin v0.11.0`. É a tag que dispara o
   `release.yml`, e é dela que sai o número — `v0.11.0` vira `0.11.0` nos três
   instaladores.
4. Gere a lista de commits para o corpo do release:
   `git log --oneline v0.10.5-1..v0.11.0 --no-merges`.
5. Cole o texto desta página no corpo, e **não apague o aviso do topo sobre a
   incompatibilidade**: ele é a única coisa que separa uma atualização tranquila
   de um grupo inteiro sem conseguir se falar.
