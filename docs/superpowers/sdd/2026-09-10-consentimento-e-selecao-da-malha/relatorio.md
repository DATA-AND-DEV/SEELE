# Consentimento e seleção dos caminhos da malha

**Data:** 2026-09-10
**Base:** `ae5b65e0aa0c79b7bc0910523ab08faf0efda781` (`main` integrada)
**Branch:** `orbita/bf86ea48`

A `main` não foi movida, nada foi publicado, nenhuma chave foi tocada, nenhum
commit foi criado, nenhum worktree de fora deste diretório foi modificado e
nenhum relatório histórico foi editado. O diff está no worktree.

---

## 1 · A base, e por que ela precisou ser incorporada

O worktree nasceu em `a695fe5` — a ponta isolada de `desenho/mods` —, **sem** a
malha. A instrução era trabalhar sobre a `main` integrada `ae5b65e`. Conferido
antes de mexer, e não suposto:

```
$ git merge-base --is-ancestor a695fe5 ae5b65e   → 0 (é ancestral)
$ git rev-list --count a695fe5..ae5b65e          → 70
$ git merge --ff-only ae5b65e
```

Como `a695fe5` é ancestral, a incorporação é **avanço rápido**: não cria commit
nenhum e não refaz merge nenhum. Sem ela, nada deste escopo existiria no
worktree — nem `crates/seele-server/src/pares.rs`, nem a spec, nem o plano.

O inventário preservado continua íntegro depois de tudo o que está abaixo:
`conferir-inventario.py` sai 0, com os 50 relatórios e o patch batendo.

### O bloqueio do claude-mem, conferido em vez de suposto

A instrução era preparar sem iniciar enquanto ele não tivesse resolução
explícita. Medido antes de editar: `consecutiveFailures: 0`, último sucesso
**posterior** ao último erro, worker, supervisor e o índice vetorial de pé, e a
própria carga de memória desta sessão voltou com dados. As duas sessões externas
em estado `blocked` são sessões do Claude Code no checkout principal, e não o
claude-mem: coisa diferente, e não controlada daqui.

### As sessões externas, conferidas antes de qualquer sobreposição

O checkout principal (`/Users/dev-alexandre/SEELE`, branch `desenho/mods`) está
**limpo**: nenhum arquivo não commitado, nada em curso sobre os arquivos deste
escopo — que, naquele ponto da história, nem existem. Nada foi escrito lá, e
nenhuma sessão foi controlada.

## 2 · O achado, e ele é do próprio §5 da spec

O §5 do desenho de 05/09 dá **duas razões independentes** para o opt-in da
malha. A implementação anterior tinha só a segunda.

- razão **2 (custo)** — a máquina de quem empresta sobe cópias para outras
  pessoas. Implementada: `EmprestarSubida { emprestando: bool }`.
- razão **1 (privacidade)** — «numa malha, espectadores passam a conhecer o
  endereço IP uns dos outros». **Não implementada para quem assiste.**

`ServerMessage::SirvaTelaPara` leva a quem empresta o endereço **de quem
assiste**, para ele discar de volta (§3.2.1 da spec). Esse endereço era
publicado sem quem assiste ter escolhido nada: o opt-in de **outra pessoa** — a
de emprestar — é que destrancava a exposição do dele. Quem só conectava já tinha
o endereço público registrado pelo servidor (origem da conexão) e entregue ao
primeiro par apontado para servi-lo.

## 3 · O que foi feito

### 3.1 · O consentimento vira um tipo, com duas metades independentes

`seele_proto::control::ConsentimentoDePar`:

| campo | o que destranca | quem paga |
|---|---|---|
| `pares_que_atende: u8` | subir cópias para outras pessoas, e até quantas ao mesmo tempo | a internet de quem empresta |
| `assiste_por_par: bool` | o endereço desta máquina ser **entregue** a quem for servi-la | a privacidade de quem assiste |

`0` **é** «não empresto» — dois campos deixariam existir «empresto, teto zero»,
contradição que cada lado resolveria de um jeito.

**Compartilhar a própria tela não mora aqui**: continua sendo
`ClientMessage::StartScreenShare`. As três escolhas que a casca precisa
distinguir ficam, então, distinguíveis: compartilhar a tela própria, emprestar a
conexão (`pares_que_atende > 0`) e assistir por par.

**A regra é de fio, e não disciplina de cliente.** `locais` — a topologia de
rede interna da máquina — só pode vir preenchido por quem empresta;
`ClientMessage::validate` recusa o resto. Consentir em assistir por par **não**
destranca essa publicação: quem assiste é alcançado pelo endereço público, que o
servidor já vê. Destrancar a maior com a menor daria a quem consentiu no menor o
custo do maior.

### 3.2 · A escolha respeita sala, transmissão e teto

`Pares::escolher` já filtrava por sala de voz. Ganhou duas paredes:

- **a transmissão** — um par repassa o que ele mesmo recebe. Estar na sala não é
  estar assistindo. A fonte é a sala de voz, que é quem liga e desliga os canos
  (`VoiceRoomCommand::QuemRecebe`, novo). Só quem recebe **do servidor** entra
  na conta: quem recebe por um par já está a um salto, e repassar dali seria o
  segundo salto de uma árvore que o A1 não entrega;
- **o teto** — `pares_que_atende`, contado pelas próprias nomeações do servidor
  (`Pares::quantos_atende`).

E o portão do endereço: `Pares::quem_consentiu_assistir_por_par` substitui
`declaracao_de` no ponto em que `apontar_um_par` monta `SirvaTelaPara`. Recusar
ali é cair para o servidor, que é o caminho de sempre.

### 3.3 · A retirada alcança o que já está no ar

`Pares::declarou` passou a **devolver** os repasses que a declaração nova
revoga, porque declarar e desfazer o que a declaração revoga são o mesmo ato —
separá-los daria a quem chama a chance de fazer só metade, e a metade que
sobraria é a que deixa alguém pagando por um consentimento que retirou.

- **no cliente** — `Motor::passar_a_consentir` cancela a tarefa que serve um par
  (a vaga volta pelo `Drop` de `VagaDeAtendimento`) e derruba os caminhos
  abertos, **antes** de a declaração nova sair;
- **no servidor** — `session::devolver_ao_servidor` reabre o cano de cada
  espectador órfão, na sala em que **ele** está (`Occupancy::onde_esta`, nova) e
  não na de quem retirou;
- **na saída da sala** — `Pares::quem_empresta_parou` fecha a terceira direção
  que faltava: as outras duas (a transmissão desta pessoa acabou; o que ela
  assistia acabou) já existiam, e a de **ela emprestando** não.

**Sem reativação tardia.** O servidor escolhe e difunde; a retirada viaja no
sentido contrário; as duas se cruzam no fio. O cliente recusa o pedido que chega
depois da retirada — o guarda mora nos dois lados, porque o consentimento é da
máquina que paga por ele.

### 3.4 · A API para a interface futura

`Enlace::consentir_no_caminho_entre_pares(ConsentimentoDePar)` **devolve o
consentimento que de fato valeu**. Esta versão do cliente honra no máximo um par
(`PARES_QUE_ESTA_VERSAO_ATENDE`) e baixa um teto maior antes de declará-lo;
calar essa correção faria a casca desenhar um deslizante em quatro e o servidor
trabalhar com um, sem nada dizendo qual vale.

## 4 · O contrato com os MODs

**A versão global do protocolo não subiu.** Conferido no registro de publicações
e não suposto: o release mais recente (`v0.10.5-1`, 05/09/2026, commit
`12a6401a6`) carrega `PROTOCOL_VERSION = 3`. A v4 existe só neste repositório e
nunca saiu, então mudar a forma de uma mensagem dela não quebra ninguém no ar — e
subir para v5 custaria a janela de compatibilidade sem nada em troca.

Registrado no §5.4 da spec, para a integração conjunta:

1. `PROTOCOL_VERSION` continua **4**; `COMPATIBILITY_WINDOW` continua **1**.
2. `ClientMessage::EmprestarSubida` trocou `emprestando: bool` por
   `consentimento: ConsentimentoDePar`. **A posição da variante não mudou** — é
   a posição que o `postcard` indexa, e é ela que quebraria quem já estivesse no
   ar.
3. Nenhuma variante nova em `ClientMessage` nem em `ServerMessage`.
4. `VoiceRoomCommand::QuemRecebe` é interno ao servidor e não atravessa fio.

## 5 · As provas por reversão

Cada guarda foi retirado, o teste rodado, e o código restaurado. A restauração
foi conferida por hash do diff inteiro (`git diff | git hash-object --stdin`).

| # | guarda retirado | teste | resultado |
|---|---|---|---|
| 1 | o religamento pelo servidor (`devolver_ao_servidor`) | `retirar_o_consentimento_de_assistir_por_par_devolve_a_tela_ao_servidor` | FALHOU — quem retirou ficou **sem imagem nenhuma** |
| 2 | `tarefas_de_par.parar_de_servir()` na retirada | `deixar_de_emprestar_cancela_o_repasse_que_ja_estava_em_curso` | FALHOU — a vaga não voltou antes do prazo do par |
| 3 | `tarefas_de_par.parar_de_assistir_a_tudo()` | `deixar_de_assistir_por_par_derruba_os_caminhos_de_par_abertos` | FALHOU — «deixou o caminho aberto de pé» |
| 4 | o guarda de consentimento em `assistir_por_par` | `um_pedido_para_assistir_por_par_que_chega_depois_da_retirada_nao_e_atendido` | FALHOU — relatou uma falha de rede que não aconteceu |
| 5 | a baixa do teto (`cabivel`) | `o_teto_pedido_acima_do_que_esta_versao_atende_e_baixado` | FALHOU — o teto pedido não foi baixado |

Os guardas de `Pares` e do protocolo foram escritos **em vermelho primeiro**: os
sete testes novos de `pares.rs` e os dois de `control.rs` falharam com as
mensagens que carregam, antes de existir conserto.

Faltavam dois desta tabela, e foi o que a revisão independente recusou. Eles
estão no §9, com a medida que mudou um dos testes.

### Dois testes que eu escrevi e que não prendiam nada

Escrito aqui porque foi o achado de método desta tarefa, e porque a suíte ficou
verde com eles dentro.

As duas primeiras versões dos testes dos guardas de reativação tardia
**passavam sem o guarda**. Um `Motor` de unidade não tem `Client`, então
`ponta_de_pares()` devolve `None` e as duas funções saem cedo de qualquer
maneira: o estado observado era idêntico nos dois casos, e o teste media isso
chamando-o de prova.

- o de `assistir_por_par` foi **consertado**: o que de fato difere é o relato —
  a saída por falta de ponta manda `ParFalhou { NaoAlcancou }`, a saída por
  consentimento retirado não manda nada, e não deve mandar. Reversão 4 acima;
- o de `servir_par` foi **removido**, porque naquele caminho não há nada
  observável sem um `Client`: `servir_par` não relata a ninguém, por desenho
  (§4 da spec: só quem recebe manda `ParFalhou`). Ver as limitações.

## 6 · Os testes novos

22 no total na primeira rodada, e mais 3 na rodada da revisão (§9): a suíte de
conformidade do caminho entre pares foi de 13 para 18.

| onde | quantos | o que prendem |
|---|---|---|
| `seele-proto/src/control.rs` | 2 | as duas metades do consentimento são independentes; `locais` sem quem empreste é recusado no fio |
| `seele-server/src/pares.rs` | 10 | o portão do endereço; a transmissão e o teto na escolha; a retirada dos dois lados; a saída de quem empresta; e o contra-guarda de redeclarar o mesmo |
| `seele-server/src/voice_room.rs` | 2 | quem recebe uma transmissão é perguntável de fora, e a pergunta sempre responde |
| `seele-server/src/server.rs` | 1 | onde uma pessoa está sentada |
| `seele-core/src/enlace.rs` | 5 | a recusa do pedido tardio; o cancelamento nos dois consentimentos; o contra-guarda da reconexão; a baixa do teto |
| `seele-conformance/.../tela_por_um_par.rs` | 2 + 3 | a retirada dos dois lados, de ponta a ponta, com trinta quadros byte a byte depois; e os três da rodada da revisão (§9) |

### A pré-condição que faltava, achada por uma falha de verdade

`um_parfalhou_por_impressao_desacredita_o_par_apontado_e_nao_a_vitima` reprovou
na primeira corrida do produto inteiro. **Não era intermitência:** a escolha do
par acontece **uma vez**, no `WatchScreen`, e o teste montava o cenário à mão
esperando só pelas declarações — não por quem empresta estar **recebendo** a
transmissão, que é a parede nova. A pré-condição foi escrita nos dois testes que
montam cenário à mão (`cenario_por` já esperava por ela).

## 7 · Verificações

| comando | resultado |
|---|---|
| `cargo test --workspace --all-targets --no-fail-fast` | **1799 passaram, 0 falharam**, 4 ignorados, em 74 alvos |
| `cargo test -p seele-conformance --test tela_por_um_par` | **18 passaram**, em **dezoito** corridas seguidas depois do conserto do §9.4 |
| `cargo clippy --workspace --all-targets` | **limpo** |
| `cargo fmt --all --check` | **limpo** |
| `cargo xtask check-deps` | passa — «dependency rule holds across 11 workspace crates» |
| `cargo xtask check-api` | passa — «toda a superfície de MOD ainda aponta para algo» |
| `conferir-inventario.py` | sai 0 — os 50 relatórios e o patch preservados continuam íntegros |

**Nenhuma falha pré-existente foi observada nesta base.** As três que os
relatórios de 09/09 registravam (a expulsão em `moderacao.rs`, e `fmt`/`clippy`
em `vetores_de_hash.rs`) vinham de `d562851`; em `ae5b65e` não aparecem.

## 8 · O que ficou de fora, e por quê

- **O guarda de `servir_par` não tem prova por reversão própria.** Ele é a
  segunda parede: a primeira é `Pares::escolher`, que não aponta quem declarou
  teto zero, e por isso o caminho de rede nunca chega a exercitá-lo. Sem um
  `Client` não há nada observável naquela função, e construir um `Enlace` de
  mentira exigiria um construtor que só os testes usam dentro de um tipo de
  produção — a forma exata do guarda que existe e não funciona. O que **está**
  provado é o efeito de que ele participa: a retirada encerra o repasse, de
  ponta a ponta, na reversão 1.
- **Dois achados novos, registrados e não consertados** —
  `docs/pendencias.md` #34 (a casca não sabe dizer de onde a tela vem agora) e
  #35 (o número de espectadores não é reanunciado quando alguém volta ao
  servidor). O #35 é **anterior** a esta onda e vale igual para a recuperação
  por `ParFalhou`; o conserto é da sala de voz, e ampliar o escopo para ele
  contrariava a instrução.
- **O teto maior que um não tem caminho de produção.** O servidor respeita
  qualquer `pares_que_atende`, e isso está provado por unidade; o cliente honra
  um só e baixa o resto. Subir esse número é uma constante, e é do subprojeto B.
- **Nenhuma medição real foi feita.** A separação entre o que a suíte local
  prova e o que só uma sala de mais de cinco pessoas pode dizer ficou escrita em
  `docs/teste-duas-maquinas.md`. A suíte prova que o mecanismo **funciona**; ela
  não prova que ele **alivia**, porque em `127.0.0.1` não há NAT para furar nem
  cano para economizar.
- **Estado publicado:** consultado uma vez, e só para decidir a versão do
  protocolo (§4). Nada neste relatório afirma o que está instalado em máquina
  nenhuma.
- **Nenhum impedimento por permissão.** Nenhuma verificação foi desativada e
  nenhum bloqueio foi contornado.

## 9 · A rodada da revisão independente

A revisão não aprovou a primeira rodada. Ela não contestou o comportamento —
mediu, em cópia isolada, e confirmou que a implementação faz o que diz. O que
ela recusou foi a **última cláusula do aceite**: «testes demonstram esses
comportamentos». Dois guardas estavam sem prova, e um documento afirmava
cobertura que não existia.

O método da recusa é o que esta casa pede e o que eu não tinha feito: ela
**desfez** cada guarda no ponto onde a decisão é tomada e viu que nada ficava
vermelho. Um guarda que sobrevive à própria remoção não é guarda.

### 9.1 · O endereço de quem recusa (achado 1)

`quem_recusa_assistir_por_par_continua_sendo_servido_pelo_servidor`, de ponta a
ponta. Quem assiste **declara** identidade de par — está na malha, empresta a
própria subida — e mesmo assim nega `assiste_por_par`. É esse o caso que separa
a porta nova da antiga: uma pessoa sem declaração nenhuma seria recusada antes,
por outro motivo, e não provaria nada.

Três afirmações, em cada um dos trinta quadros: nenhum `SirvaTelaPara` saiu pela
transmissão (a escuta que mede **exposição**, e não alívio), o servidor continuou
subindo as **duas** cópias, e a imagem chegou byte a byte. Recusar não é ficar
sem tela.

**Reversão:** trocando a porta pela versão de antes desta onda, o teste falha
dizendo `o endereço dele foi entregue? true; cópias que o servidor ainda sobe: 1
(esperava 2)`. A imagem para de verdade — o servidor desliga o cano para dar
lugar ao par, e a máquina de quem recusou não atende, porque o cliente também
sabe da recusa.

### 9.2 · A terceira direção, e a medida que mudou o teste (achado 2)

Aqui a revisão estava certa sobre o buraco e o conserto foi diferente do que ela
sugeriu, porque a medida disse outra coisa.

O teste sugerido — clientes de verdade dos dois lados, imagem do órfão não para
— foi escrito e **passa com e sem a linha**. Medido em 11/09: os quadros chegam
nos mesmos 34 ms e nos mesmos números de sequência nas duas versões. A razão é
que há **dois** caminhos de volta: o servidor, na própria saída, e o relato de
fim de repasse que a máquina do órfão manda ao perceber. Em `127.0.0.1` o
segundo volta dentro de um intervalo de quadro, e a tela não pisca.

Separar os dois exigiu uma ponta que **não sabe relatar**: `EspectadorCru`
recebe a tela do servidor e declara identidade de par para poder ser nomeada,
mas nunca disca nem relata. Quem empresta também não relata — por protocolo, só
quem assiste manda `ParFalhou`. O que reabre o cano dessa ponta só pode ser o
servidor, sozinho, na saída. A medida é a **contagem de aberturas de fluxo de
tela** para ela: reabrir é um evento, e um booleano não diria se voltou ou se
nunca caiu.

**Reversão:** sem a linha, o cano nunca reabre — a ponta fica sem imagem e
ninguém vai relatar nada. Com ela, 69 ms.

O teste de clientes de verdade **ficou**, com a limitação escrita no próprio doc
dele: ele não prende aquela linha, prende os **dois** caminhos de uma vez. É a
diferença entre medir um mecanismo e medir a promessa, e o arquivo precisa dos
dois.

### 9.3 · A frase de cobertura que não conferia (achado 3)

`docs/teste-duas-maquinas.md` afirmava que a suíte provava a recusa. Depois de
9.1 ela prova; a lista foi reescrita para dizer o que cada item prende, com uma
frase nova: **cada afirmação daquela lista foi provada por reversão**, e é esse
o filtro que a separa de uma lista de nomes de teste. A limitação de 9.2 entrou
como quarto item do «o que ela não diz».

### 9.4 · Um vermelho falso, achado por acidente e consertado

Repetindo a suíte para conferir estabilidade, duas corridas em vinte reprovaram
na montagem do cenário, esperando o servidor «contar as duas cópias». Não era
lentidão: o contador assinava o barramento **depois** de abrir a transmissão, e
o servidor anuncia o número de espectadores **uma vez**. Quem assina tarde perde
o anúncio único e fica em `u32::MAX` para sempre — trinta segundos de paciência
gastos sem nada de errado ter acontecido.

Reproduzido de propósito com trezentos milissegundos de atraso entre as duas
linhas: falha todas as vezes, com a mesma mensagem das corridas reais. Com a
assinatura movida para antes da transmissão existir, passa com o atraso ainda
lá. Depois disso, dezoito corridas seguidas da suíte inteira sem uma falha.

Vale dizer o que isto foi: um teste que acusava o produto de um defeito que era
dele mesmo. É o mesmo defeito de método do §5 desta casa, na direção oposta —
lá um teste que passava sem o conserto, aqui um que falhava com ele.
