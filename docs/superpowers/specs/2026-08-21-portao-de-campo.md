# Portão de campo — o que medir, e o que o ciclo não sabe

**Data:** 2026-08-21
**Estado:** aguardando execução

O ciclo de conectividade fechou onze commits. **Tudo o que ele mudou foi medido
contra `127.0.0.1`.** O defeito que o originou — o furo abrindo por 600 ms e o
aperto de mão chegando de 4 a 12 segundos depois — só se manifesta com dois
roteadores de verdade, e é isso que este teste existe para descobrir.

## O que mudou, e o que cada coisa deveria fazer no campo

| mudança | o que ela deveria produzir lá |
|---|---|
| O `LEVE` sai colado em cada candidato que precisa dele, e se repete três vezes enquanto o aperto de mão corre | o furo do anfitrião abre **enquanto** o `Initial` está chegando, e não onze segundos antes |
| `PACOTES_DO_FURO` caiu de 5 para 1 | nenhuma diferença visível; a cobertura temporal agora vem do aviso, não da repetição |
| `FUROS_POR_JANELA` subiu de 20 para 60 | duas ou três pessoas entrando juntas param de fechar a janela uma contra a outra |
| Candidato privado de outra casa vale 1 s em vez de 4 s | o pior caso de um convite morto cai de ~16 s para ~3 s |
| A escada declara o degrau a partir dos alvos que sobraram | uma casa com duas placas e IPv6 nativo para de anunciar `FuroDeNat` sem o endereço furado no convite |
| A porta do roteador entrou na reserva | uma casa com privacy extensions para de **perder o mapeamento UPnP** e cair para o degrau 4 |
| `EnderecoDireto` existe | uma VPS para de ler «este link só funciona na sua rede» |

## O roteiro, e por que a ordem importa

O roteiro anterior falhou não por falta de zelo, mas porque **exibia sucesso e
fracasso com a mesma cara**: o relato voltou como «não conectou», e nada nele
separava NAT simétrico de UDP bloqueado, de ponto de encontro mudo, ou de furo
fora de hora.

1. **`plug --rede` nas duas máquinas, antes de qualquer tentativa.** Guardar a
   saída inteira das duas. Duas saídas coladas classificam o caso sem ninguém
   adivinhar — e é a única etapa que não existia antes.
2. **Hospedar com o UPnP desligado.** Registrar a frase impressa, o link inteiro,
   e a ordem dos candidatos.
3. **Colar o link na máquina B.** Registrar a **trilha carimbada** da `Chegada`:
   qual candidato, em que instante, com que desfecho. É o entregável central do
   ciclo, e o que substitui «não conectou».
4. **Na VPS, `--barulhento` com carimbo:** o instante de cada `ONDE`, `LEVE` e
   `AQUI`.
5. **Conectado:** `:sync` nos dois, e **por qual caminho a sessão saiu**.
6. Ponto de encontro fora do ar, e queda de rede — os testes de que o degrau 4
   não virou ponto único de falha.
7. **A máquina B sem IPv6**, com um convite construído à mão com quatro
   candidatos `::ffff:a.b.c.d` privados. Mede a escolha da família da sonda, que
   é a única propriedade do ciclo sem falsificador automático: ~4 s com a
   canonização, ~16 s sem ela.

## O que este ciclo sabe que **não** sabe

**A aposta central não foi medida.** `ESPERA_DO_FURO = 200 ms` contra um RTT real
de 20 a 200 ms até o ponto de encontro. Se ela for curta demais, o `Initial` chega
antes do `FURO` e o degrau 4 volta a depender de um PTO do quinn.

**A linha `entrada de fora` nunca vai dizer «chega» de uma máquina cujo NAT
reescreve porta.** A medida exata custa um segundo ponto de encontro noutra
máquina: o ouvinte aprenderia o próprio endereço pelo ponto B e o `LEVE` sairia
para o ponto A, que nunca falou com ele.

**`Nat::Simetrico` só tem teste puro.** No laço local os dois pontos veem o mesmo
socket. Há caminho para exercitá-lo numa máquina só — um relê UDP de teste com um
socket por destino, ~25 linhas — e ele não foi feito. Não é impossível; é não
feito.

**O modo par usa uma marca constante.** Duas execuções simultâneas na mesma rede
se confundiriam, e `esperar_o_par` aceita o furo de qualquer origem.

**NAT simétrico dos dois lados continua sem saída**, por decisão do ADR 0022. Se o
teste cair nesse caso, o resultado é a informação, não o fracasso.

## A frase que governa este teste

Da seção 20 do documento que originou o ciclo, e vale repetir porque é o que
separa um relato útil de uma tarde perdida:

> O objetivo não é fazer todos os casos funcionarem. O objetivo é identificar
> precisamente quais casos funcionam e quais não funcionam.

Um relato utilizável é: duas saídas de `plug --rede`, a trilha carimbada, o
`--barulhento` da VPS, e a operadora e o modelo de roteador de cada lado. Com
isso, «não conectou» vira «o furo abriu às 12:03:01.4 e o `Initial` saiu às
12:03:05.6» — que é uma linha de código, não um mistério.

## Uma instabilidade vista uma vez, e não reproduzida

Ao rodar os portões antes do último commit, `cargo test --workspace` reprovou uma
vez em `seele-conformance --test acceptance_seguranca` — 6 de 7. O binário
sozinho passou quatro vezes seguidas, e o `--workspace` seguinte passou inteiro,
então não há nome de teste para nomear aqui: a saída da falha não foi capturada
antes de a re-execução limpar o rastro.

Fica registrado porque **é um teste de segurança**, e porque este projeto já
gastou uma sessão perseguindo instabilidade que ninguém tinha anotado. A hipótese
mais provável é disputa de porta ou de relógio entre binários de teste rodando em
paralelo — a suíte sobe Dogmas de verdade em `127.0.0.1`. Quem for atrás: rode
`cargo test --workspace` sob carga e capture a saída com `--nocapture` na primeira
reprovação.
