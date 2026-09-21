# A escolha de qualidade no compartilhamento: piso, teto e quadros

> **Escopo.** A caixa de compartilhar passa a perguntar resolução **e** quadros,
> e a resolução escolhida passa a ser o **piso** da estimativa de caminho além
> de continuar sendo o teto de qualidade. A escada adaptativa continua inteira,
> e continua sendo a única coisa que baixa a qualidade.

## A frase que este documento implementa

*«A resolução coloca um piso inicial de subida, para não ter aquele tempo de
adaptação que tem nos degraus, assim como um teto máximo de qualidade para
todos os espectadores. Ainda vai existir os degraus para reduzir a qualidade.»*

---

## O que se mede hoje, e por que a escolha não vale nada no começo

A resolução escolhida é **só teto**. `crates/seele-core/src/video.rs:890` faz
`menor_resolucao(degrau, escolha)`: a escada decide o degrau, e a escolha só
pode baixá-lo. Escolher 1080p numa sessão nova não entrega 1080p.

Quem prende é a perna do hospedeiro. A corrente, medida:

1. O servidor manda `HostUplink { bps: caminho_no_fio(declarado, medido) }`.
2. `caminho_no_fio` devolve `medido`, senão `declarado`, senão **0** — a
   hipótese não atravessa o fio, por desenho.
3. O cliente faz `caminho_de_quem_hospeda_bps = (bps > 0).then_some(bps)`, e
   zero vira `None`.
4. Com `None`, `teto_de_video` não chama `com_caminho_de_quem_hospeda`, e vale
   o padrão de `TetoDeVideo`: `CAMINHO_DA_PROVA_BPS`, **2 Mbps**.

Dois Mbps dão teto de 1,2 Mbps, que compra 540p. O 1080p pede 6.240.000 bps de
teto, ou seja **10,4 Mbps** de caminho com um espectador. Da escada, são 8
janelas de um segundo a 25% para sair de 2 e passar de 10,4 — 3 numa LAN —, e na
prática mais, porque nem toda janela enche.

**Corolário que fecha um caso aberto:** a v0.14.0 subiu
`CAMINHO_DO_SERVER_BPS` de 2 para 8 Mbps «para permitir os perfis de resolução
escolhidos no cliente». Ela não podia ter permitido nada: aquele número governa
a sonda e o portão **deste lado**, e nunca chega ao cliente. Ele foi revertido
noutra volta, com guarda próprio, porque cegava a medida.

---

## A decisão

**O piso sai da escolha, e nunca sobrepõe o que a máquina já sabe.**

Quando o `HostUplink` traz um número maior que zero — medido ou declarado —, ele
manda, para mais **e** para menos. Só na ausência dele a escolha vira piso.

Foi decidido contra duas alternativas:

- **Partir do escolhido sempre**, ignorando medida lembrada. Recusado: repetiria
  um palpite que a máquina já desmentiu.
- **Só pular a escada com base real.** Recusado: não resolve a primeira
  transmissão de um servidor novo, que é o caso relatado.

---

## O contrato do cliente

Um lugar, em `crates/seele-core/src/enlace.rs`. `teto_de_video` passa a receber
os limites em vez de só a banda, e a perna do hospedeiro ganha padrão:

```rust
let hospeda = self
    .caminho_de_quem_hospeda_bps
    .unwrap_or_else(|| limites.caminho_inicial_bps());
teto = teto.com_caminho_de_quem_hospeda(hospeda);
```

`caminho_inicial_bps()` já existe em `LimitesDeTela` e é a **mesma** função que a
perna de quem compartilha já usa: não há segundo mapa para divergir no dia em
que um dos dois mudar.

### Mas os três números dela têm de ser derivados, e hoje são escritos à mão

Ela devolve 3, 5 e 8 Mbps, escritos à mão. Conferidos contra a escada que os
consome:

| escolha | piso à mão | teto (60%) | a escada compra |
| --- | --- | --- | --- |
| 540p | 3 000 000 | 1 800 000 | 540p |
| 720p | 5 000 000 | 3 000 000 | 720p |
| 1080p | 8 000 000 | 4 800 000 | **720p** |

**O piso de 1080p não compra 1080p.** O limiar são 6 240 000 bps de teto, que
pedem 10 400 000 de caminho, e 8 000 000 ficam 2,4 Mbps abaixo. Quem escolhesse
1080p continuaria em 720p — o pedido inteiro desta entrega, não atendido, em
silêncio.

É o defeito que o doc de `TETO_ESTIMADO_PARA_1080P_BPS` já nomeia sobre si
mesmo: *«6 200 000 à mão, 10 000 abaixo do que a divisão de `cadencia_para`
precisa (…) Duas constantes arredondadas em separado divergem; uma derivada da
outra não pode.»* A mesma armadilha, o mesmo arquivo, um mês depois.

Então `caminho_inicial_bps` passa a **derivar** do limiar que a escada usa:

```
piso = limiar_da_resolucao × 100 / FRACAO_DO_CAMINHO
```

| escolha | piso derivado | teto (60%) | a escada compra |
| --- | --- | --- | --- |
| 540p | 2 600 000 | 1 560 000 | 540p |
| 720p | 4 650 000 | 2 790 000 | 720p |
| 1080p | 10 400 000 | 6 240 000 | 1080p |

Os três passam a comprar o que prometem, **por construção** — e no dia em que
um limiar mudar, o piso o acompanha sozinho. Isto conserta a perna de quem
compartilha junto, onde o mesmo 8 Mbps já estava curto desde que foi escrito.

Fora de uma transmissão não há escolha, e o padrão segue `CAMINHO_DA_PROVA_BPS`.

### O que **não** muda

- **O fio.** `caminho_no_fio` continua recusando pôr a hipótese nele. O cliente
  supõe por conta própria; ninguém mente na rede.
- **O servidor.** `CAMINHO_DO_SERVER_BPS` continua em 2 Mbps, governando a
  sonda e o portão. O portão não confere resolução: ele recusa quando não sobram
  `PISO_DE_BANDA_BPS` por cópia, o que a 2 Mbps admite seis cópias.
- **`mediu`**, e a medida que ele libera para o fio e para o disco.
- **A escada.** Ela continua sendo a única coisa que baixa a qualidade, e
  continua descendo sozinha quando o cano não compra o que foi pedido.

---

## O contrato da interface

### Quadros na caixa

Um `<select id="compartilhar-quadros">` ao lado do de resolução, com **30 e 60**.
`quadros_maximos` passa a lê-lo em vez da constante `60`, e o ouvinte de
`change` que hoje serve só à resolução passa a servir aos dois — a troca vale
durante a transmissão, como a de resolução já vale.

`Cadencia` tem `Q8` e `Q15` também, e eles ficam **fora da lista**: existem como
piso automático, para onde a rede desce pela regra «a resolução segura, o quadro
cede». Oferecer 8 a quem escolhe é pedir que a pessoa escolha «propositalmente
travado».

Nada em Rust: `limites_do_nucleo` já traduz qualquer `quadros_maximos` para o
maior degrau de `Cadencia` que cabe.

### O padrão passa a ser 720p

Com o piso, o padrão deixa de ser uma preferência e vira uma **suposição sobre a
casa de quem hospeda**. A 1080p ele suporia 8 Mbps de todo mundo que nunca
abrisse a caixa; numa casa modesta são segundos de perda e de pressão sobre a
reserva de voz até a escada puxar de volta — e o §3.2 diz que a voz nunca cede à
tela. Arranhá-lo por padrão, e não por escolha de quem usa, é o que esta troca
recusa.

A 720p o piso são 5 Mbps e o teto 3 Mbps: acima do limiar de 720p
(2.790.000 bps) e abaixo do que uma casa modesta não carrega. Quem quer 1080p
escolhe, e aí o palpite é dela.

---

## A malha: nada a fazer

`crates/seele-core/src/par.rs` é **só transporte** — um par serve uma vaga e
repassa o fluxo, sem recodificar. «A malha copia as configurações do host» já é
verdade por construção, e «teto máximo para todos os espectadores» também: quem
compartilha codifica uma vez, e servidor e pares encaminham os mesmos bytes.

O que um par empresta é subida, não qualidade. Esta entrega não toca nisso.

---

## Testes

**Unidade, em `seele-core`:**

- **Cada piso compra a resolução que promete.** Para os três degraus:
  `resolucao_para(fracao_do(caminho_inicial_bps()), Nitidez)` devolve a
  resolução escolhida. É o guarda que teria pego o 8 Mbps curto, e é o irmão de
  `cada_limiar_compra_a_resolucao_que_promete`, que já existe para a outra
  ponta da mesma tabela.
- Sem `HostUplink`, a perna do hospedeiro sai da escolha.
- Com `HostUplink` maior que zero, ele vence — provado nas **duas** direções,
  acima e abaixo do piso da escolha. É a regra inteira da decisão, e um teste só
  do lado alto deixaria passar o caso que importa: a medida baixa que desmente o
  palpite.
- Sem transmissão não há escolha, e o padrão segue `CAMINHO_DA_PROVA_BPS`.

**Bancada, em Chromium com o `index.html` real:**

- O select de quadros aplicado **durante** a transmissão, como
  `ajustes-v013.cjs` já faz com o de resolução.
- O padrão da caixa é 720p.

**Texto-fonte, em `frontend.rs`:** que `limitesEscolhidos` **lê** o select de
quadros, e não carrega um número fixo — o espelho do guarda que já existe para
`prioridade`.

Cada guarda é provado por reversão: desfazer o conserto e ver o teste falhar,
antes de dizer que ele guarda alguma coisa.

---

## O que esta entrega deliberadamente não faz

- **Não separa `CAMINHO_DO_SERVER_BPS` em dois números.** A separação só faz
  falta se o portão estiver recusando transmissões que caberiam, e a 2 Mbps ele
  admite seis cópias. Fica para quando alguém medir que ele atrapalha.
- **Não põe a subida declarada na interface.** `config.caminho_bps` já atravessa
  o fio e já vence o piso; falta só o campo. É a resposta certa para quem sabe
  quanto tem, e é outra entrega — esta existe justamente para não perguntar
  Mbps a quem não sabe.
- **Não faz o piso ceder mais rápido que a escada.** A primeira janela que
  reclama já derruba a estimativa pelas regras que existem. Encurtar isso é
  mexer na sonda, e a sonda acabou de custar uma bateria reprovada.
- **Não oferece 8 e 15 quadros**, pelo motivo escrito acima.
