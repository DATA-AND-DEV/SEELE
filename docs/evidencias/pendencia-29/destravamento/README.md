# A fila com prazo: o que acontece quando quem tem a vez trava

**Por que existe.** Serializar troca uma reprovação isolada por uma fila, e uma
fila tem um modo de falha que a permissão não tinha antes: quem toma a vaga e
**nunca a devolve** para todos os 138 testes atrás de si. Não é hipótese — a
própria §29 documenta esse travamento como causa 2: sob acesso restrito o
CoreAudio não recusa, ele espera. A revisão independente desta entrega apontou o
risco, e este diretório é a resposta.

**O conserto, em duas partes — e a segunda só existe porque a medida cobrou.**

1. `vaga::minha()` deixou de esperar no `Condvar` sem prazo. Ela acorda de cinco
   em cinco segundos e mede **não o tempo que esperou** — o último da fila espera
   legitimamente a suíte inteira, que leva minutos — e sim o tempo **sem nenhuma
   devolução**. Três minutos parados e ela escreve no erro padrão que vai seguir
   sem serializar, e segue.
2. A desistência é **da fila, não de quem esperou**. A primeira versão deixava
   cada teste descobrir o travamento por conta própria, e a medida mostrou o
   preço disso: como quem fura a fila devolve a vaga ao terminar, o relógio de
   «sem progresso» reiniciava para todos os outros, e a fila soltava **um teste
   por prazo, em cascata**. Com os 180 s entregues e 138 testes atrás de um
   travado, isso seria um travamento com outro nome. Agora quem desiste marca a
   fila como abandonada e acorda todo mundo: a rodada volta a ser paralela de
   uma vez só.

**Como foi provado.** O módulo real (`crates/seele-conformance/tests/vaga/mod.rs`)
foi compilado fora do `cargo`, com duas trocas de número e nenhuma de lógica:
prazo de 180 s para 3 s e passo de 5 s para 200 ms, para caber num registro. Um
dono toma a vaga e nunca a devolve (`std::mem::forget`); **oito** testes entram
na fila atrás dele. `prova.rs` é o programa, e é o mesmo nos três registros — só
o módulo muda.

| Registro | Permissão | Resultado |
| --- | --- | --- |
| `com_prazo.log` | como está entregue | os oito seguem **juntos**, aos 3,35 s; saída **0** |
| `cascata.log` | desistência individual (a versão anterior) | um por prazo: 3,36 / 6,40 / 9,46 / 12,52 / 15,58 / 18,64 s; saída **0** |
| `sem_prazo.log` | `DEVOLVEU.wait(fila)`, sem prazo (`vaga_sem_prazo.rs`) | **ninguém** segue; o vigia do programa mata aos 30 s; saída **9** |

A terceira linha é a prova de reversão: com a espera sem prazo de volta, a fila
não anda nunca — é o que aconteceria com a suíte inteira. A segunda é a prova de
que a desistência **coletiva** não é enfeite: oito testes custam 18,64 s de
cascata contra 3,35 s de uma vez, e a conta cresce linearmente com o tamanho da
fila.

**Uma honestidade sobre o registro.** Em `com_prazo.log` a mensagem de
desistência aparece duas vezes, não uma: duas threads bateram no prazo no mesmo
instante, antes de qualquer uma marcar a fila. Isso é benigno — as duas tomam a
vaga que já iam tomar — e o registro fica como saiu, sem edição.
