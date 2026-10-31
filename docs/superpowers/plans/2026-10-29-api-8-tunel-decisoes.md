# API 8, parte 2 (túnel) — decisões e pendências

Registro das decisões tomadas durante a execução de
`2026-10-29-api-8-tunel.md` (commits `6e33ce3d..fa7a53ac`) e do que ficou para
depois. Cada decisão diz o custo, se estiver errada.

## Decisões

- **R1.** A `api/v8.json` e a linha dela em `congeladas.sha256` puderam ser
  refeitas: a v0.18.1 (`ae0f2715`) não tem `api/v8.json`, e as v0.18.2/0.18.3
  dizem no corpo que a API 8 não entra. *Custo:* mudar uma API publicada.
- **R2.** O `j` reusa `seele_enlace::Recusa {Grande, Cheia, Fechada}`, com o
  texto generalizado para voz e túnel. *Custo:* um enum para dois usos; separar
  depois é refatoração local.
- **R3.** `quem_sai_do_servidor_fecha_a_porta` confere a porta recusando e o
  fim do fluxo na máquina; `PessoaSaiu` é motivo só do log do servidor, e não
  uma `Queda`. *Custo:* o motivo do log fica sem asserção nesse teste.
- **R4.** A bancada `tunel-de-jogador.cjs` entra no `ci.yml` e no
  `publicar.sh`. *Custo:* tempo de CI.
- **R5.** O plano se contradizia (trocar os eventos da tela por um só *versus*
  `tela_por_um_par.rs` sem nenhuma linha mudada). Valeu o arquivo intocado: a
  tela segue com `Event::SirvaTelaPara`/`AssistaTelaPor`, e o túnel usa
  `Event::Apresentacao`. *Custo:* dois formatos de evento até a tela migrar.
- **R6.** `AlcanceTunelPor` ganhou `quem: PersonId` (nasceu no 12, neste ramo,
  e nunca foi publicada); só a linha dela em `fio_2_0.txt` foi trocada. Serve
  para «ligação direta com <apelido>». *Custo:* se o congelamento valer para
  linhas nascidas no ramo, voltar a uma busca impressão→pessoa.
- **R7.** `RecusaDeTunel` ganhou, no fim, `PortaLocalOcupada` e `SemResposta`,
  locais como `ServidorAntigo`; as três são recusadas no `Validate` ao
  decodificar. *Custo:* três ordinais que nunca viajam.
- **R8.** Descarregar um MOD fecha os túneis da sessão a que ele pertencia,
  pelo carimbo, e a faixa diz «fechada porque você trocou de servidor».
  *Custo:* trocar a vista fecha a porta do jogo da sessão de fundo.
- **R9.** `ServerConfig::servir_tuneis`, padrão `false`: sem ele, `ServirTunel`
  é recusado com `SemPermissao`. A parte 3 troca isso pelo aceite da máquina.
  *Custo:* nenhum servidor de produção serve túnel até a parte 3.
- **R10.** Mensagem nova no fim da lista, `TunelRetirado { mod_id, nome, quem }`
  (desde 12), para a máquina saber que alguém perdeu o túnel e cortar o
  direto. *Custo:* mais uma mensagem do 12 para congelar.
- **R11.** Todo datagrama `j` leva um byte de marca (abertura | dado); quem
  abre repete a abertura até a primeira volta (e ao menos a cada 1 s); dado
  antes da abertura é contado, e não corta. *Custo:* 1 byte por datagrama.
- **R12.** Ficam para a parte 3 em diante: teto de canais repassados por
  pessoa; as medidas na tela do MOD (spec §3 — desvio registrado);
  `MotivoDoFimDoTunel::AnfitriaoCaiu` documentado como reservado.
- **Teste instável vigiado.** `revogar_entrar_no_jogo_corta_o_direto_na_maquina`
  supõe que o `TunelRetirado` chega à máquina antes do fecho do próprio
  jogador (5/5 aqui); vigiar no CI.
- **Desvio aceito (M1).** A sonda curinga usa `SO_REUSEADDR` no Unix (porta
  em TIME_WAIT não parece ocupada, medido no Mac); EACCES não tem nome próprio.

## Pendências menores (não bloqueiam)

Resolvidas pela onda final, e só listadas para o registro: o `porque` da v8,
o dono de `ServirTunel` (R9), as perdas do `j` (R11), o doc de
`Event::TunelRecusado`, as recusas locais decodificáveis (R7), o «direto»
com fluxos ainda na estrela, a segunda entrada no mesmo túnel, e
`Permitidos` que só crescia (R10).

Abertas:

- **T1:**
  - o nome do teste `…dez_e_onze…`, e nenhum teste fixa os carimbos 11/12 nem
    `negociar(Faixa{10,11})`;
  - o teto de plataformas é um 2 literal, e `[App, App]` passa;
  - as linhas `tunel:*` estão no meio do `fio_2_0.txt` (só acréscimos).
- **T2:**
  - as violações de protocolo do núcleo/web não foram provadas por reversão;
  - uma asserção repetida em `frontend.rs`;
  - um `catch` mudo em `camada-mods.js`;
  - o mapeamento Plataforma→nome está em dobro (web/ffi);
  - um `sleep` de 200 ms num servidor de mentira.
- **T3:**
  - `Poder::Programa` ainda não tem regra (fica para a parte 4);
  - nenhum teste liga `capacidades-por-poder` ao código;
  - uma linha longa no README.
- **T4:**
  - as entradas `tunel_descartado`/`tunel_grande_demais` não têm teste;
  - um `j` que chega ordenado não é recusado;
  - as quedas do desmonte também contam como descarte;
  - questões de justiça entre o `j` e os outros canais;
  - um `Vec` alocado por chamada.
- **T5:**
  - o teste do par 11 não discrimina a barreira de eventos;
  - não há prova de ponta a ponta de `CanaisEsgotados`.
- **T6:**
  - o `Tela` de `MotivoDaApresentacao` e `par::por_onde` estão mortos em
    produção;
  - um comentário duplicado em `translate`;
  - o log de «vai pela estrela» não diz a causa;
  - um teste de ordem que não falha;
  - logs INFO em excesso.
- **T7:**
  - `udp_descartados` conta a conexão inteira, e não o túnel;
  - o ciclo de vida do lado da máquina (`parar`, `servidos`, `tarefas`);
  - sair com o pedido pendente não o cancela;
  - a metade «IP da interface» do teste de loopback não foi provada por
    reversão;
  - não há teste do fechamento por 60 s parado;
  - não há teste de ponta a ponta de `Direto.par`.
- **T8:**
  - o teste de dois túneis não espera o direto;
  - o contador de `Controle` não é legível;
  - uma segunda apresentação com outra impressão é absorvida;
  - `Caiu` vira `NaoAlcancou`;
  - `podar` só roda em algumas saídas;
  - a conferência do UDP lê um relatório possivelmente velho.
- **T9:**
  - a bancada liga o fecho sozinha;
  - falta confirmar a experiência da linha «fechou…»;
  - não há teste com duas sessões reais e uma troca de vista;
  - o carimbo fica vazio se o MOD sobe sem vista;
  - fechar depois de reconectar vira `falhou`.
- **Final:**
  - um teste novo em `frontend.rs` ficou entre o doc e o `fn` de outro;
  - a guarda M6 da FFI pode esconder por ≤1 s um túnel novo de mesma chave;
  - a segunda conferência de `atender_entrar` descarta os `Encerramento` de
    outros túneis do mesmo MOD.
