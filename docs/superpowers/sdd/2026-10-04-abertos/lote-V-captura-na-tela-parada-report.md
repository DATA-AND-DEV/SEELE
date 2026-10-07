# Lote V-captura-na-tela-parada — relatório

Base `d744644`. Commits locais, nada empurrado:

- `24ba9f5` A captura do macOS decide entre «sem conteúdo» e «converter» numa função pura, e o estado desconhecido com imagem ganha teste de unidade que roda sem tela
- `96d3a42` O teste de campo da captura do macOS separa a tela parada da captura travada e de quem lê parado, e a tela parada passa dizendo isso com os números

Arquivo mudado: `crates/seele-video/src/captura/macos.rs` (só ele).

## A medida antes de mexer

O teste reprovou na base, como o K1 mediu:

```
a captura entregou 17 quadros em três segundos ... escritos pelo sistema: 17
```

Antes de escrever o teste novo, medi o que a ScreenCaptureKit entrega nesta máquina (macOS 27.0.1, `screencapturekit`
8.0.1). Usei um teste temporário com um `SCStreamOutputTrait` que conta `(frame_status(), image_buffer().is_some())`. Ele
não entrou em commit, e o arquivo voltou byte a byte (`cmp`). O resultado, com três segundos por monitor:

| monitor | amostras | por segundo | estado / imagem |
|---|---|---|---|
| 2 (1080×1920, externo, o 1º da lista) | 88 | 30, 29, 29 | `None`/com imagem 17, `None`/sem imagem 71 |
| 5 (2560×1440) | 88 | 30, 29, 29 | `None`/com imagem 88 |
| 1 (1512×982) | 88 | 29, 29, 30 | `None`/com imagem 25, `None`/sem imagem 63 |

Duas coisas saem daí:

- O `frame_status()` vem `None` nas 264 amostras. A tela parada chega como amostra **sem imagem**, e não como `Idle`:
  aqui, o «sem conteúdo» vem do `image_buffer()` vazio.
- As amostras chegam a ~30 por segundo com a tela parada ou não. A frase antiga («o ScreenCaptureKit reentrega o mesmo
  quadro») é falsa: ele não reentrega, ele manda amostras sem imagem.

## Item 1 — a decisão vira função pura

**O que fiz.** `classificar<I>(estado: Option<SCFrameStatus>, imagem: Option<I>) -> Destino<I>`, com
`enum Destino<I> { SemConteudo, Converter(I) }`. O tratamento da amostra chama
`classificar(amostra.frame_status(), amostra.image_buffer())`. O comentário do defeito dos 145 quadros mudou-se para o
doc da função, com a medida de 05/10/2026 acrescentada.

**Por que `SCFrameStatus` e não um enum espelho.** O tipo do crate é um `enum` público, com variantes públicas e
`from_raw(i32) -> Option<Self>`, e não é `#[non_exhaustive]`. Ele se constrói num teste sem nada da ScreenCaptureKit
de pé. O espelho seria uma segunda lista para manter igual à primeira.

**Por que `Option<I>` e não `tem_imagem: bool`.** Com um `bool`, o braço «converter» ainda teria de tirar a imagem do
`Option` e tratar um `None` que não acontece, ou o `converter` teria de continuar procurando a imagem, com um `Ok(None)`
que repete a decisão. Com o genérico, o `Converter(imagem)` já carrega o `CVPixelBuffer`, e a decisão mora num lugar só.
Nos testes, `I = &str`.

**A consequência no `converter`.** Ele passa a receber `&CVPixelBuffer` e a devolver `Result<QuadroI420, _>`. O
`let Some(buffer) = amostra.image_buffer() else { return Ok(None) }` e o `.map(Some)` saíram, porque essa era a metade da
decisão que morava nele. O corpo da conversão não mudou. Os contadores (`sem_conteudo`, `ilegiveis`, `por`) recebem o
mesmo que antes para cada combinação de estado e imagem.

**Testes** (rodam em qualquer Mac, sem TCC):
- `o_estado_desconhecido_com_imagem_e_convertido`: `None`, `from_raw(42)` e `from_raw(-1)`, com imagem, dão
  `Converter`;
- `os_estados_sem_conteudo_nao_sao_convertidos`: `Idle`, `Blank`, `Suspended` e `Stopped` dão `SemConteudo`;
- `os_estados_com_conteudo_sao_convertidos`: `Complete` e `Started` dão `Converter`;
- `sem_imagem_nao_ha_o_que_converter`: sem imagem, com `None`, `Complete`, `Started` ou `Idle`, dá `SemConteudo`.

As seis variantes do crate estão cobertas. Se uma versão nova do `has_content()` mudar de opinião sobre alguma delas,
um teste fica vermelho.

**RED/GREEN.** RED: os testes escritos antes da função não compilaram (`cannot find function classificar`,
`cannot find type Destino`). GREEN: 4 passaram depois da extração.

**Reversões**, feitas contra o arquivo final, restauradas por cópia e conferidas com `cmp` (sha `fb557bbf5cf5`):
- R1a, o defeito antigo (`None` vira «sem conteúdo»): `if !estado.is_some_and(|e| e.has_content())`. Ficou vermelho
  `o_estado_desconhecido_com_imagem_e_convertido` com «uma amostra com imagem e estado desconhecido (None) foi contada
  como «sem conteúdo»: é o defeito que já descartou 145 quadros seguidos sem uma linha de erro, com a captura parecendo
  funcionar e não entregando nada».
- R1b, só `Idle` como sem conteúdo: ficou vermelho `os_estados_sem_conteudo_nao_sao_convertidos` com «o estado Blank
  diz que a amostra não traz quadro novo da tela, e ela foi mandada converter: a vaga receberia como quadro novo o que
  o sistema disse que não é».
- R1a também no campo, com a tela de verdade: `a_captura_entrega_quadros_ao_longo_do_tempo` ficou vermelho com
  «as amostras chegaram, e nenhum quadro com imagem chegou a quem lê em 3 segundos. […] Medido: amostras por segundo
  [29, 28, 29] (escritos [0, 0, 0], sem conteúdo [29, 28, 29])». O `captura_um_quadro_da_tela_de_verdade` também
  ficou vermelho («um quadro chega em três segundos»). Nesta máquina o estado é sempre `None`, e por isso o defeito
  inteiro aparece também no teste de campo. Num sistema que mande o estado, só o teste de unidade o pega.

## Item 2 — o teste ao longo do tempo cobra o que pode provar

**O que fiz.** Tudo no `mod testes` (código de teste, sem produção):
- `medir(vaga, tomar)`: lê a vaga a cada 5 ms por três segundos e anota, em cada segundo, `escritos`, `sem_conteudo` e
  `pegos`. Anota também os `descartados` e os `ilegiveis` da janela. O teste de campo passa
  `|| captura.tomar()`, e os simulacros passam a vaga direto ou um `tomar` que para.
- `julgar(&Medida) -> Result<Veredito, String>`, com as regras nesta ordem, para que a falha nomeie a causa e não um
  efeito dela:
  1. nenhuma amostra ilegível;
  2. em **cada** segundo, `escritos + sem_conteudo >= 10` (`MINIMO_DE_AMOSTRAS_POR_SEGUNDO`, um terço dos ~30 medidos):
     uma captura que trava reprova, com a tela parada ou não;
  3. pelo menos um quadro com imagem pego por quem lê;
  4. em todo segundo com 3 escritos ou mais, quem lê pegou pelo menos um;
  5. com 20 pegos ou mais, `TelaQueMuda`; com menos e `sem_conteudo > escritos`, `TelaParada`; com menos e a tela
     mudando, falha.
- O teste de campo imprime no `stderr` `TELA PARADA em <monitor>: …` com os números quando a tela ficou parada, e
  `MEDIDO em <monitor>: …` quando ela mudou. A falha é `panic!("{monitor:?}: {motivo}")`. A frase falsa saiu, e o doc do
  teste conta a medida que a desmentiu.
- O primeiro monitor da lista continua sendo o usado. Escolher o principal era opcional, e não o fiz: assim o caminho
  da tela parada continua exercitado nesta máquina.

**Por que as regras 1, 4 e o «falha» da 5, que a lista não pedia.**
- A 1 mantém a frase honesta. Sem ela, uma captura toda ilegível sairia como «parou de entregar amostras», porque as
  ilegíveis não entram em `escritos + sem_conteudo`, e umas poucas ilegíveis passariam caladas. O teste irmão
  (`captura_um_quadro…`) já cobra `ilegiveis == 0`.
- A 4 cobre a reversão escrita na lista, «um `tomar()` que para de entregar depois do primeiro segundo». Os contadores
  são da vaga, e não de quem lê. Se só o `tomar()` parar e a captura continuar escrevendo, as regras 2 e 3 passam
  (`pegos` ≈ 30 no primeiro segundo). A 4 pega isso.
- O «falha» da 5 impede que a medida diga «TELA PARADA» quando os números mostram a tela mudando: poucos pegos, muitos
  escritos e poucas amostras sem conteúdo. Ali, o mínimo de 20 de antes continua valendo.

**Testes** (unidade, sem TCC): `a_tela_parada_passa_e_e_dita` (os números do K1, `[17,0,0]` escritos e as 73 sem
conteúdo repartidas em 13/30/30), `a_tela_que_muda_passa`, `um_segundo_sem_amostras_reprova`,
`nenhum_quadro_com_imagem_reprova`, `quem_le_e_para_de_receber_reprova`, `poucos_quadros_com_a_tela_mudando_reprova` e
`uma_amostra_ilegivel_reprova`.

Há também dois simulacros, que juntam `medir` e `julgar` (uma thread escreve na vaga a cada 10 ms; ~3,2 s cada, em
paralelo):
- `uma_captura_que_para_depois_do_primeiro_segundo_reprova`: o simulacro escreve por 1 s e para. Exige
  «parou de entregar amostras»;
- `um_tomar_que_para_depois_do_primeiro_segundo_reprova`: o simulacro escreve o tempo todo, e o `tomar` devolve `None`
  depois de 1 s. Exige «quem lê não pegou nenhum».

O simulacro escreve a cada 10 ms, e não a 30 por segundo, para que uma máquina ocupada pela bateria não o derrube
abaixo do mínimo de 10 e troque o motivo da falha que os testes conferem.

**RED/GREEN.** RED: na base, o teste de campo reprovava nesta máquina com a frase falsa (acima). Os testes novos,
escritos antes de `medir`, `julgar`, `Medida`, `Segundo` e `Veredito`, não compilaram (22 erros, `cannot find
function julgar` etc.). GREEN: os 9 testes novos e o de campo passaram. O de campo deu `TELA PARADA`.

**Reversões** contra o arquivo final, cada uma restaurada por cópia e conferida com `cmp` (sha `fb557bbf5cf5`). Cada
reversão deixou vermelho exatamente o teste dela:
- R2a, sem a regra 1 (`ilegiveis > u64::MAX`): `uma_amostra_ilegivel_reprova`, «uma amostra com pixels que não deu
  para converter passou: um formato de pixel inesperado ficaria escondido atrás de uma medida verde: TelaQueMuda».
- R2b, sem a regra 2 (`amostras() < 0`): `um_segundo_sem_amostras_reprova`, «uma captura que deixou de mandar amostras
  no terceiro segundo passou: a captura travada voltaria a passar como tela parada: TelaQueMuda»; e o simulacro
  `uma_captura_que_para…`, «uma captura que parou de entregar depois do primeiro segundo passou pela medida de campo:
  TelaQueMuda».
- R2c, sem a regra 3 (`pegos == u64::MAX`): `nenhum_quadro_com_imagem_reprova`, «noventa amostras, todas sem conteúdo,
  passaram: é o defeito que já descartou 145 quadros seguidos, contado como tela parada: TelaParada».
- R2d, sem a regra 4 (`pegos == u64::MAX`): `quem_le_e_para_de_receber_reprova`, «a captura escreveu trinta quadros
  por segundo e quem lê só os recebeu no primeiro segundo, e a medida passou: TelaQueMuda»; e o simulacro
  `um_tomar_que_para…`, «um tomar() que parou de entregar depois do primeiro segundo passou pela medida de campo, com a
  captura escrevendo quadros o tempo todo: TelaQueMuda».
- R2e, sem o «falha» da regra 5 (`sem_conteudo >= 0`): `poucos_quadros_com_a_tela_mudando_reprova`, «quinze quadros
  pegos com noventa escritos e nenhuma amostra sem conteúdo passaram: a tela parada não explica isso, e a medida
  passaria dizendo que explica: TelaParada».
- R2f, a tela parada devolvendo `TelaQueMuda`: `a_tela_parada_passa_e_e_dita`, «uma tela parada, com as amostras
  chegando sem conteúdo a cada segundo, não foi reconhecida como tela parada: o teste de campo reprova numa máquina
  cujo primeiro monitor não mudou, ou passa sem dizer por quê (…)».
- R-campo, a captura de verdade parada depois do primeiro segundo. Dentro do teste de campo, o `tomar` chama
  `captura.fluxo.stop_capture()` em 1 s. Ficou vermelho com «Fonte(Monitor 2 (1080×1920)): a captura parou de
  entregar amostras: no 2º segundo chegaram 0, contando os quadros com imagem e as amostras sem conteúdo, e o mínimo é
  10 em cada um dos 3 segundos (…)».

**O que a reversão «um `tomar()` que para», feita no teste de campo, não pode mostrar nesta máquina.** Com o monitor
parado, depois do primeiro segundo não há quadro escrito para o `tomar()` entregar (`escritos [16, 0, 0]`). Um `tomar()`
que para ali é indistinguível de um que funciona, e nenhum teste os separa. Por isso essa reversão mora no simulacro
(`um_tomar_que_para…`), onde a captura continua escrevendo. A versão «a captura para» foi feita no campo (R-campo).

## Item 3 — `cargo test -p seele-video` várias vezes nesta máquina

Todas as rodadas deram verde: 59 testes de unidade passaram, 3 ignorados (já eram), e os 5 binários de teste do
pacote ficaram verdes. Os números são do teste de campo (o 1º monitor, o externo 1080×1920):

| rodada | veredito | amostras/s | escritos/s | sem conteúdo/s | pegos/s |
|---|---|---|---|---|---|
| 1 | tela que muda | 30, 30, 29 | 19, 0, 25 | 11, 30, 4 | 20, 0, 25 |
| 2 | **TELA PARADA** | 29, 29, 30 | 18, 0, 0 | 11, 29, 30 | 19, 0, 0 |
| 3 | tela que muda | 30, 29, 29 | 30, 29, 29 | 0, 0, 0 | 30, 30, 29 |
| 4 | tela que muda | 30, 28, 29 | 30, 28, 29 | 0, 0, 0 | 30, 29, 29 |
| 5 | tela que muda | 30, 29, 29 | 30, 29, 29 | 0, 0, 0 | 30, 30, 29 |
| 6 | **TELA PARADA** | 30, 29, 30 | 18, 0, 0 | 12, 29, 30 | 19, 0, 0 |
| 7 | **TELA PARADA** | 30, 29, 30 | 16, 0, 0 | 14, 29, 30 | 17, 0, 0 |
| 8 | **TELA PARADA** | 29, 30, 29 | 15, 0, 0 | 14, 30, 29 | 16, 0, 0 |
| 9 | tela que muda | 30, 29, 30 | 17, 20, 0 | 13, 9, 30 | 18, 20, 0 |
| 10 | **TELA PARADA** | 30, 29, 30 | 18, 0, 0 | 12, 29, 30 | 19, 0, 0 |
| 11 | tela que muda | 30, 29, 29 | 18, 2, 0 | 12, 27, 29 | 19, 2, 0 |
| 12 | tela que muda | 29, 30, 29 | 24, 1, 0 | 5, 29, 29 | 25, 1, 0 |
| 13 | tela que muda | 29, 30, 29 | 18, 1, 0 | 11, 29, 29 | 19, 1, 0 |
| 14 | tela que muda | 29, 30, 29 | 24, 1, 0 | 5, 29, 29 | 25, 1, 0 |

Descartados 0 e ilegíveis 0 em todas. As rodadas 1 a 10 rodaram sobre a versão cuja diferença para o HEAD é só de texto
(três frases de doc e de mensagem, corrigidas na autorrevisão), e as rodadas 11 a 14 sobre o conteúdo do HEAD `96d3a42`.
Fora da tabela, três rodadas só do teste de campo deram `TELA PARADA` com escritos `[16, 0, 0]` e pegos 17.

O monitor «parado» nem sempre está parado. Nas rodadas 3 a 5, algo nele mudou o tempo todo. Nas rodadas 9 e 11 a 14
mudou um pouco, e com 20 pegos ou mais a medida dá «tela que muda». O que conta: com ele parado (2, 6, 7, 8, 10) o teste
passa e diz isso, e a base reprovava exatamente nesse caso.

## Verificação

- `cargo fmt --all -- --check`: limpo antes de cada commit.
- `cargo clippy --workspace --all-targets --all-features -- -D warnings`: limpo antes de cada commit (só o aviso
  ambiental `ld: duplicate -rpath`).
- `cargo test -p seele-video`: as 14 rodadas acima, mais as pontuais.
- A versão intermediária (`24ba9f5`) compila, passa no clippy, e passa nos 4 testes novos. Nela o teste de campo
  continua reprovando nesta máquina como na base, e o `96d3a42` conserta isso.
- Os 14 assuntos do G1 (plano 1B, linhas 2839-2852) foram conferidos por `grep -F` contra os dois assuntos: nenhum
  aparece.
- A bateria inteira não rodou aqui: é o passo próprio do fluxo do lote.

## O que não pôde ser compilado nem rodado aqui

Nada deste lote é de outra plataforma. O `captura/macos.rs` é `cfg(target_os = "macos")` e compilou e rodou aqui, e o
`captura/windows.rs` não foi tocado. Os testes novos de unidade e os simulacros rodam em qualquer Mac, inclusive num
runner de CI sem TCC. Os de campo pulam em voz alta sem permissão de gravação de tela (`PULADO: …`), como antes.

## Autorrevisão

- Cada frase nova foi conferida contra o medido ou contra o código:
  - os 264 `None` e o «perto de trinta por segundo» vêm da medida da tabela de cima;
  - «uns 17 no primeiro segundo» vem do K1 (17) e das rodadas (15 a 19 escritos);
  - «de 28 a 30» vem da medida (29-30) e das rodadas (28-30);
  - o `from_raw` para valores desconhecidos e negativos devolve `None`, lido em `screencapturekit-8.0.1/src/cm/frame_status.rs` e `sample_buffer.rs:111`;
  - o `has_content()` dá `Complete | Started`.
- Três frases foram corrigidas antes de entregar:
  - «o `CMSampleBuffer` só existe com uma captura de pé» era falso, porque o crate constrói um a partir de um
    `CVPixelBuffer`. Virou «cujo anexo de estado só a ScreenCaptureKit escreve»;
  - «17 quadros» virou «uns 17»;
  - «de 29 a 30» virou «de 28 a 30».
- Para que os commits já levassem as frases certas, refiz os dois commits locais (`b46e18f` e `49a2b8b` viraram
  `24ba9f5` e `96d3a42`) por `git reset --mixed d744644` e dois commits de novo. Nada tinha sido empurrado, e as
  reversões foram refeitas contra o conteúdo final.
- Nenhum `unwrap`/`expect` novo fora de teste, e nenhum índice em `src/`. Nenhuma dependência nova. O fio, a API e a
  CSP não foram tocados.

## Preocupações

1. **Mudança de produção além da função extraída, no limite do «só extrair».**
   - O `converter` mudou de assinatura: recebe `&CVPixelBuffer` e devolve `Result<QuadroI420, _>`.
   - O `image_buffer()` passou a ser lido **antes** do estado. Numa amostra `Idle`/`Blank`/`Suspended`/`Stopped` que
     traga imagem, ele faz agora um retain e um release que antes não fazia.
   - Os contadores e o que vai para a vaga são os mesmos para cada combinação. Nesta máquina o estado é sempre `None`,
     e o caminho é idêntico.

   Se o revisor preferir a assinatura `tem_imagem: bool` da lista, o custo é um `None` impossível no braço
   «converter», ou o `Ok(None)` duplicado no `converter`.
2. **O `julgar` tem mais regras do que a lista pediu** (a 1, a 4 e o «falha» da 5). O motivo está no item 2. Cada uma
   tem teste e reversão.
3. **Testes que dependem do relógio.**
   - Os dois simulacros levam ~3,2 s cada, em paralelo.
   - A regra 4 supõe que quem lê não fica um segundo inteiro sem rodar.
   - O mínimo de 10 amostras por segundo supõe que a ScreenCaptureKit não cai abaixo de um terço da cadência.

   Nada disso foi medido sob a carga da bateria inteira (`cargo test --workspace`), que é o passo seguinte do fluxo.
   Se reprovar lá, a mensagem diz qual regra e com quais números.
4. **Pegos de 19 a 21 no monitor quase parado** ficam na fronteira entre `TelaParada` e `TelaQueMuda`. As duas
   passam: muda o rótulo do `stderr`, não o resultado.
5. O teste de campo continua no primeiro monitor da lista (o opcional não foi feito), de propósito: assim o caminho da
   tela parada continua exercitado aqui.
