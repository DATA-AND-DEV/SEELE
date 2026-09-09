# Onda final — diff 725be01..HEAD

## Commits
004322a docs(spec): as assinaturas do §4 param de contradizer o parágrafo acima delas
103f1b3 docs(enlace): o caminho de empréstimo diz que não tem chamador de produção
464fe41 docs(pares): a suposição sobre `stable_id` fica escrita onde a invariante mora
64a9b01 fix(proto): a impressão digital passa a ser conferida inteira, e não só o teto
d71993a docs(par): o `# Errors` de `ligar` passa a listar os cinco motivos que ela dá
4408ea8 fix(par): quem empresta para de desistir quando a própria discagem falha
ecfe474 fix(pares): todo caminho que encerra um repasse devolve o par à fila
6cd8d80 fix(sessao): as duas mensagens da v4 param de sair para um cliente v3
36e75a3 fix(enlace): o repasse passa a saber de qual tela ele é, e recusa a segunda
eb96701 fix(enlace): o fim limpo de um fluxo de par também avisa quem tem de assumir
fcadb2e fix(pares): a escolha de um par passa a ficar dentro da sala de voz da tela
005b18d fix(teste): o contador de cópias continua contando depois de o barramento atrasar

## Stat
 crates/seele-conformance/tests/tela_por_um_par.rs  | 305 +++++++++++++++++-
 crates/seele-core/src/enlace.rs                    | 349 +++++++++++++++++++--
 crates/seele-core/src/par.rs                       |  23 +-
 crates/seele-proto/src/control.rs                  | 120 ++++++-
 crates/seele-server/src/pares.rs                   | 280 ++++++++++++++++-
 crates/seele-server/src/session.rs                 | 214 +++++++++++--
 .../specs/2026-09-05-caminho-entre-pares-design.md |  57 +++-
 7 files changed, 1245 insertions(+), 103 deletions(-)

## Diff
diff --git a/crates/seele-conformance/tests/tela_por_um_par.rs b/crates/seele-conformance/tests/tela_por_um_par.rs
index 2fc724b..885e7ed 100644
--- a/crates/seele-conformance/tests/tela_por_um_par.rs
+++ b/crates/seele-conformance/tests/tela_por_um_par.rs
@@ -403,34 +403,56 @@ async fn esperar<T, F: FnMut(&Aviso) -> Option<T>>(
 /// `VoiceRoom::copias` —, e o servidor o anuncia a cada mudança pelo
 /// [`Event::ScreenViewers`]. Uma cópia que o servidor não sobe é uma cópia que
 /// só pode ter vindo de outro lugar.
 struct ContadorDeCopias {
     quantos: Arc<std::sync::atomic::AtomicU32>,
 }
 
 impl ContadorDeCopias {
     /// Passa a seguir o contador desta transmissão.
     fn de(servidor: &Daemon, screen: ScreenId) -> Self {
+        Self::seguindo(servidor.server().events.subscribe(), screen)
+    }
+
+    /// O mesmo, a partir de uma assinatura já feita do barramento.
+    ///
+    /// Existe separado de [`Self::de`] para que
+    /// [`o_contador_de_copias_sobrevive_a_um_atraso_do_barramento`] possa
+    /// entregar um recebedor **já atrasado** — coisa que um daemon de verdade
+    /// não sabe produzir sob encomenda.
+    fn seguindo(mut eventos: tokio::sync::broadcast::Receiver<Event>, screen: ScreenId) -> Self {
         let quantos = Arc::new(std::sync::atomic::AtomicU32::new(u32::MAX));
         let escrevendo = Arc::clone(&quantos);
-        let mut eventos = servidor.server().events.subscribe();
         tokio::spawn(async move {
-            while let Ok(evento) = eventos.recv().await {
-                if let Event::ScreenViewers {
-                    screen: qual,
-                    quantos,
-                    ..
-                } = evento
-                {
-                    if qual == screen {
-                        escrevendo.store(quantos, std::sync::atomic::Ordering::Relaxed);
+            loop {
+                // **`Lagged` não encerra a contagem.** O barramento do servidor
+                // larga eventos quando quem lê fica para trás, e um `while let
+                // Ok(..)` trataria essa perda como fim de barramento: o laço
+                // sairia e `agora()` congelaria no último número visto. Um
+                // contador congelado em `1` afirma para sempre a única coisa
+                // que estes testes existem para provar. Perder eventos custa
+                // precisão; parar de ler custa a prova.
+                match eventos.recv().await {
+                    Ok(evento) => {
+                        if let Event::ScreenViewers {
+                            screen: qual,
+                            quantos,
+                            ..
+                        } = evento
+                        {
+                            if qual == screen {
+                                escrevendo.store(quantos, std::sync::atomic::Ordering::Relaxed);
+                            }
+                        }
                     }
+                    Err(tokio::sync::broadcast::error::RecvError::Lagged(_)) => continue,
+                    Err(tokio::sync::broadcast::error::RecvError::Closed) => break,
                 }
             }
         });
         Self { quantos }
     }
 
     /// O último número que o servidor anunciou. `u32::MAX` enquanto não houve
     /// nenhum — um valor que nenhuma asserção deste teste aceita por engano.
     fn agora(&self) -> u32 {
         self.quantos.load(std::sync::atomic::Ordering::Relaxed)
@@ -725,25 +747,32 @@ async fn quando_o_par_morre_o_servidor_assume_e_ninguem_perde_imagem() -> Result
         |aviso| match aviso {
             Aviso::TelaQuadro { tela, bytes, .. } if *tela == screen => {
                 seq_de(bytes).map(|seq| (seq, ()))
             }
             _ => None,
         },
     )
     .await?;
     println!("o quadro {pelo_par} chegou pelo par; agora quem empresta morre");
 
-    // **À força, e não com `sair()`.** Uma despedida limpa é o caso fácil: o
-    // servidor vê a sessão acabar e podia limpar sozinho. O que a propriedade
-    // promete é o caso difícil — a máquina de alguém sumindo —, e é ele que
-    // este `drop` produz: `Enlace::drop` aborta a tarefa que fala com o
-    // servidor e larga a conexão QUIC no meio de uma transmissão.
+    // **À força, e não com `sair()`.** O que este teste cobre é a máquina de
+    // alguém sumindo: `Enlace::drop` aborta a tarefa que fala com o servidor e
+    // larga a conexão QUIC no meio de uma transmissão, e do lado de quem
+    // assiste isso chega como um fluxo cortado — um erro de leitura.
+    //
+    // **A despedida limpa não é o caso fácil**, e este comentário já disse
+    // que era. Um fluxo de par que termina direito é indistinguível, para
+    // quem assiste, de uma transmissão que acabou — e quem assiste já saiu do
+    // cano do servidor desde que o par foi apontado, então calar ali era tela
+    // em branco permanente. O caso limpo tem prova própria, em
+    // `o_fim_limpo_do_repasse_devolve_quem_assiste_ao_servidor`, e o conserto
+    // dele está no `Ok(None)` de `escoar_tela_alheia`.
     drop(empresta);
 
     // O quadro que prova a promessa é um **posterior** ao último que o par
     // entregou. Um que já estivesse na fila de avisos não provaria nada.
     let alvo = pelo_par + 1;
     let (seq, bytes) = esperar(
         &mut assiste,
         "um quadro chegar pelo servidor depois de o par morrer",
         |aviso| match aviso {
             Aviso::TelaQuadro { tela, bytes, .. } if *tela == screen => seq_de(bytes)
@@ -919,10 +948,256 @@ async fn um_parfalhou_por_impressao_desacredita_o_par_apontado_e_nao_a_vitima()
         );
         tokio::time::sleep(Duration::from_millis(20)).await;
     }
 
     drop(compartilha);
     drop(empresta);
     drop(assiste);
     servidor.shutdown();
     Ok(())
 }
+
+/// **Um repasse que termina limpo devolve quem assiste ao servidor.**
+///
+/// O irmão de `quando_o_par_morre_o_servidor_assume_e_ninguem_perde_imagem`, e
+/// o caso que ele **não** cobre. Lá o fluxo do par é cortado no meio e a
+/// leitura devolve erro; aqui ele termina direito — `finish()` do outro lado,
+/// `Ok(None)` desta — com a transmissão ainda no ar.
+///
+/// # Por que o caso limpo não é o caso fácil
+///
+/// Porque quem assiste **não tem como distinguir** «a tela acabou» de «o par
+/// calou»: as duas chegam como um fluxo que termina sem erro. E o cano do
+/// servidor para esta pessoa foi desligado quando o par foi apontado
+/// (`TelaParouDeAssistir`), então calar aqui é tela em branco permanente, sem
+/// ninguém saber.
+///
+/// # Como o fim limpo é produzido
+///
+/// Quem empresta para de assistir. O cano do servidor para ele fecha, a tarefa
+/// que lê a tela alheia dele chega ao fim do fluxo, `RepasseDeTela::fechou`
+/// desliga o destino, e `par::repassar` termina o fluxo do par direito — com a
+/// transmissão de quem compartilha continuando no ar para todo mundo. É um dos
+/// três caminhos do fim limpo (os outros são a contrapressão de
+/// `PEDACOS_A_ESPERA_DO_PAR` e quem empresta sair da sala), e é o único que um
+/// teste produz sem tocar em relógio nem em memória.
+#[tokio::test(flavor = "multi_thread")]
+async fn o_fim_limpo_do_repasse_devolve_quem_assiste_ao_servidor() -> Result<()> {
+    let Cenario {
+        servidor,
+        compartilha,
+        empresta,
+        mut assiste,
+        screen,
+        copias,
+    } = cenario().await?;
+
+    // **O repasse tem de estar mesmo no ar antes de acabar.** Uma versão
+    // anterior deste teste pedia «o primeiro quadro» e mandava quem empresta
+    // parar de assistir logo depois: o quadro era o `seq` 0, que já estava na
+    // fila desde antes da malha, e quem empresta parava **antes** de a ligação
+    // com o par sequer fechar. O que o teste media então era o caminho de
+    // `AssistaTelaPor` sem par nenhum do outro lado — não o fim limpo de um
+    // repasse. A prova de que o par está servindo é a mesma do teste central:
+    // um piso drenado, e um segundo de imagem acima dele com o servidor
+    // subindo uma cópia só.
+    let piso_inicial = maior_seq_ja_enfileirado(&mut assiste, screen).await;
+    assiste.assistir(screen, true).await?;
+    ate("o servidor parar de subir a cópia de quem assiste", || {
+        copias.agora() == 1
+    })
+    .await?;
+    let mut piso = piso_inicial;
+    for indice in 0..QUADROS_PARA_PROVAR {
+        let (seq, _) = esperar(
+            &mut assiste,
+            "um quadro chegar pelo par, acima do piso, com o cano do servidor desligado",
+            |aviso| match aviso {
+                Aviso::TelaQuadro { tela, bytes, .. } if *tela == screen => seq_de(bytes)
+                    .filter(|seq| piso.is_none_or(|p| *seq > p))
+                    .map(|seq| (seq, ())),
+                _ => None,
+            },
+        )
+        .await?;
+        assert_eq!(
+            copias.agora(),
+            1,
+            "o servidor voltou a subir a cópia de quem assiste antes de o repasse começar de \
+             verdade (quadro {indice} de {QUADROS_PARA_PROVAR}, seq {seq})"
+        );
+        piso = Some(seq);
+    }
+    println!("o repasse pelo par está no ar até o quadro {piso:?}; agora ele termina limpo");
+
+    // **Sem `drop`, e sem erro nenhum.** Quem empresta continua conectado, na
+    // sala e vivo; só deixa de assistir. O repasse acaba pelo caminho educado.
+    empresta.assistir(screen, false).await?;
+
+    // O fim do fluxo do par, visto de dentro de quem assiste. Daqui para
+    // frente, tudo o que chegar tem de ter vindo de outro lugar.
+    esperar(&mut assiste, "o fluxo do par terminar", |aviso| {
+        matches!(aviso, Aviso::TelaFechou { tela } if *tela == screen).then_some(())
+    })
+    .await?;
+
+    // **Um piso novo, e é ele que faz esta prova valer.** O canal de avisos é
+    // FIFO e o par entregou quadros até calar; pedir um `seq` acima do último
+    // que se leu passaria com o defeito no lugar, servido pela fila. Drenar
+    // até a fila esvaziar é o que separa «o servidor voltou a servir» de
+    // «ainda havia imagem velha guardada».
+    let depois_do_par = maior_seq_ja_enfileirado(&mut assiste, screen).await;
+    println!("a fila de quem assiste esvaziou no quadro {depois_do_par:?}");
+
+    // E sustentação, pela mesma razão do teste central: um quadro isolado
+    // acima do piso ainda cabe num punhado em voo; um segundo inteiro de
+    // imagem, cada quadro acima do anterior, não cabe.
+    let mut anterior = depois_do_par;
+    for indice in 0..QUADROS_PARA_PROVAR {
+        let (seq, bytes) = esperar(
+            &mut assiste,
+            "um quadro chegar pelo servidor depois de o repasse ter terminado limpo",
+            |aviso| match aviso {
+                Aviso::TelaQuadro { tela, bytes, .. } if *tela == screen => seq_de(bytes)
+                    .filter(|seq| anterior.is_none_or(|p| *seq > p))
+                    .map(|seq| (seq, bytes.clone())),
+                _ => None,
+            },
+        )
+        .await?;
+        assert_eq!(
+            bytes,
+            corpo(seq),
+            "o quadro {seq} chegou e não é o que saiu de quem compartilha (quadro {indice} de \
+             {QUADROS_PARA_PROVAR})"
+        );
+        anterior = Some(seq);
+    }
+    println!(
+        "{QUADROS_PARA_PROVAR} quadros seguidos chegaram pelo servidor depois do fim limpo do \
+         repasse, todos acima de {depois_do_par:?}"
+    );
+
+    drop(compartilha);
+    drop(empresta);
+    drop(assiste);
+    servidor.shutdown();
+    Ok(())
+}
+
+/// **Um repasse encerrado normalmente devolve o par à fila.**
+///
+/// A nomeação do servidor (`Pares::apontou`) só era desfeita por um
+/// `ParFalhou`. Quando o repasse terminava **bem** — quem assiste fecha a
+/// janela e manda `UnwatchScreen` —, ela ficava de pé, e `Pares::ja_servindo`
+/// contava aquele par como ocupado pelo resto da sessão do daemon. Do lado do
+/// cliente `atendendo_pares` já tinha sido devolvido: os dois lados
+/// discordavam em silêncio, e a malha degradava para a estrela — um par por
+/// transmissão encerrada — sem um único rastro dizendo por quê.
+///
+/// A afirmação é sobre o **estado do servidor**, e não sobre imagem: é lá que
+/// a vaga era queimada, e é lá que o teste tem de olhar.
+#[tokio::test(flavor = "multi_thread")]
+async fn um_repasse_encerrado_normalmente_devolve_o_par_a_fila() -> Result<()> {
+    let Cenario {
+        servidor,
+        compartilha,
+        empresta,
+        assiste,
+        screen,
+        copias,
+    } = cenario().await?;
+
+    let empresta_quem = empresta.sessao().person;
+
+    assiste.assistir(screen, true).await?;
+    ate("o servidor parar de subir a cópia de quem assiste", || {
+        copias.agora() == 1
+    })
+    .await?;
+    {
+        let pares = servidor.server().pares.lock().await;
+        assert!(
+            pares.ja_servindo().contains(&empresta_quem),
+            "o servidor apontou um par e não o contou como ocupado"
+        );
+    }
+
+    // **Quem assiste sai da sala**, e é este o caminho que nenhuma mensagem do
+    // cliente cobre. Um `UnwatchScreen` também encerra o repasse, mas depois
+    // do conserto do fim limpo o `ParFalhou` que ele provoca já solta a
+    // nomeação pelo braço de sempre — reverter a linha do `UnwatchScreen` não
+    // faz teste nenhum falhar. A saída da sala não tem esse socorro: o
+    // cliente não relata nada, e se o servidor não soltar a nomeação sozinho
+    // ela fica de pé para sempre.
+    assiste.sair_da_voice_room().await?;
+
+    let fim = Instant::now() + PACIENCIA;
+    loop {
+        let ocupados = servidor.server().pares.lock().await.ja_servindo();
+        if ocupados.is_empty() {
+            break;
+        }
+        assert!(
+            Instant::now() < fim,
+            "o repasse terminou bem e o par continua contado como ocupado ({ocupados:?}) — \
+             o servidor nunca mais vai escolhê-lo, e o cliente dele já devolveu a vaga"
+        );
+        tokio::time::sleep(Duration::from_millis(20)).await;
+    }
+    println!("o par voltou à fila depois de o repasse ter sido encerrado normalmente");
+
+    drop(compartilha);
+    drop(empresta);
+    drop(assiste);
+    servidor.shutdown();
+    Ok(())
+}
+
+/// **O contador não pode calar quando o barramento atrasa.**
+///
+/// `ContadorDeCopias` é a metade negativa de toda prova deste arquivo: um
+/// `copias.agora() == 1` afirmado trinta vezes seguidas. Se o laço que o
+/// alimenta sair do ar num `Lagged` — e o barramento do servidor larga eventos
+/// por desenho quando alguém não os lê a tempo —, `agora()` congela no último
+/// número que viu. Congelado em `1`, ele afirma trinta vezes uma coisa que
+/// parou de conferir, e o servidor pode ter voltado a subir a cópia sem que
+/// asserção nenhuma acuse.
+///
+/// É o mesmo falso-verde que a Task 10 caçou por outra porta. Aqui ele é
+/// provado direto: um recebedor que **já perdeu** eventos, e um número que
+/// chega depois da perda.
+#[tokio::test(flavor = "multi_thread")]
+async fn o_contador_de_copias_sobrevive_a_um_atraso_do_barramento() -> Result<()> {
+    let screen = ScreenId(7);
+    let voice_room = VoiceRoomId(1);
+
+    // Um barramento minúsculo, cheio **antes** de alguém ler: a primeira coisa
+    // que o laço do contador encontra é o `Lagged`.
+    let (fala, ouve) = tokio::sync::broadcast::channel::<Event>(2);
+    for quantos in 0..4 {
+        fala.send(Event::ScreenViewers {
+            voice_room,
+            screen,
+            quantos,
+        })
+        .expect("o recebedor está vivo");
+    }
+
+    let copias = ContadorDeCopias::seguindo(ouve, screen);
+
+    // O número que importa vem **depois** do atraso. Um contador que desistiu
+    // no `Lagged` nunca o vê.
+    fala.send(Event::ScreenViewers {
+        voice_room,
+        screen,
+        quantos: 1,
+    })
+    .expect("o recebedor está vivo");
+
+    ate(
+        "o contador enxergar o número que veio depois do atraso do barramento",
+        || copias.agora() == 1,
+    )
+    .await?;
+    Ok(())
+}
diff --git a/crates/seele-core/src/enlace.rs b/crates/seele-core/src/enlace.rs
index ffac2b7..deb1a84 100644
--- a/crates/seele-core/src/enlace.rs
+++ b/crates/seele-core/src/enlace.rs
@@ -2977,24 +2977,29 @@ impl Motor {
 ///
 /// Não devolve nada e não avisa o servidor de falha nenhuma, de propósito:
 /// `seele_proto::control::ClientMessage::ParFalhou` é explícita que só quem
 /// **recebe** manda essa mensagem. Um repasse que morre no meio é imagem que
 /// para do lado de lá, e é de lá que o aviso sai.
 async fn repassar_a_tela(repasse: &RepasseDeTela, ligado: &par::ParLigado, screen: ScreenId) {
     // Sem abertura não há o que repassar: ou nenhuma transmissão está chegando
     // agora, ou ela acabou entre o pedido do servidor e a ligação fechar. Um
     // par ligado num fluxo sem cabeçalho não decodifica nada, e mandar-lhe
     // pedaços soltos seria pior que não mandar nada.
-    let Some(abertura) = repasse.abertura() else {
+    // `abertura_de` e não `abertura`: com duas transmissões no ar, a que está
+    // sendo repassada pode não ser a que o servidor mandou servir. Repassar a
+    // outra seria entregar ao par uma tela com o nome de outra — ver o doc de
+    // `EstadoDoRepasse::qual`.
+    let Some(abertura) = repasse.abertura_de(screen) else {
         tracing::warn!(
             ?screen,
-            "o par ligou e não há transmissão nenhuma chegando do servidor para lhe repassar"
+            "o par ligou e esta máquina não está recebendo do servidor a transmissão que lhe \
+             mandaram repassar; quem assiste continua sendo servido pelo servidor"
         );
         return;
     };
     let (pedacos_tx, pedacos_rx) = mpsc::channel(PEDACOS_A_ESPERA_DO_PAR);
     repasse.ligar(pedacos_tx);
     let resultado = par::repassar(ligado, &abertura, pedacos_rx).await;
     repasse.desligar();
     match resultado {
         Ok(()) => tracing::info!(?screen, "o repasse desta tela ao par terminou"),
         Err(erro) => tracing::warn!(%erro, ?screen, "o repasse desta tela ao par falhou"),
@@ -3073,47 +3078,93 @@ async fn servir_um_par(
     let atende = par::atender(ponta.clone(), PRAZO_DO_PAR);
     let disca = par::ligar(
         &ponta,
         &enderecos,
         impressao,
         Some(&identidade),
         PRAZO_DO_PAR,
     );
     let resultado = tokio::select! {
         atendido = atende => atendido,
-        discado = disca => discado.ok(),
+        // **O fim da discagem não é uma resposta**, e tratá-lo como uma era
+        // desistir de servir alguém que estava chegando. Quem assiste nunca
+        // chama `par::atender` — só disca —, então a discagem **desta** ponta
+        // não tem quem a atenda e não pode fechar. Ela existe por um efeito
+        // só, que é metade do §3.3: abrir o mapeamento de NAT deste lado para
+        // a discagem do outro entrar. Quando ela erra cedo — família de
+        // endereço incompatível, `connect_with` recusando na hora, todos os
+        // candidatos falhando rápido —, o braço que a esperava cancelava
+        // `atender` e devolvia `None`.
+        //
+        // O prazo global continua sendo o de `par::atender`, que é o mesmo
+        // [`PRAZO_DO_PAR`]: este braço nunca resolve, então é sempre o outro
+        // que termina a espera.
+        () = discagem_so_pelo_furo(disca) => None,
     };
     // **Desarma sempre, sirva ou não sirva.** Achado do fix round 3: sem
     // isto a ponta continuava aceitando conexões pelo resto da sessão — ver
     // o doc de [`par::parar_de_atender`]. A função já drena e recusa
     // sozinha qualquer sobra da corrida acima antes de desarmar de verdade —
     // ver o doc dela para o pânico que isso evita — porque o perigo mora no
     // `set_server_config(None)` que ela faz, não neste ponto de chamada: um
     // `servir_um_par` de amanhã sem essa linha não devia poder reabri-lo.
     par::parar_de_atender(&ponta).await;
     resultado
 }
 
+/// A discagem de quem empresta, que abre o furo e não decide nada.
+///
+/// **Nunca resolve, de propósito.** Um `select!` que espera esta função espera
+/// só o outro braço; o que esta metade faz é manter a discagem viva enquanto
+/// [`par::atender`] tem prazo, pelo efeito de abrir o mapeamento de NAT desta
+/// ponta. O resultado dela vai para o rastro e para lugar nenhum mais: quem
+/// assiste nunca atende, então uma discagem que «deu certo» aqui seria uma
+/// surpresa, e uma que falhou é o esperado.
+async fn discagem_so_pelo_furo<F>(disca: F)
+where
+    F: std::future::Future<Output = Result<par::ParLigado, par::ErroDePar>>,
+{
+    match disca.await {
+        // Não é o caminho de produção — quem assiste não atende —, mas se um
+        // dia for, largar a conexão aqui é o certo: é `atender` que decide.
+        Ok(_) => tracing::debug!("a discagem de quem empresta fechou; quem decide é o atendimento"),
+        Err(erro) => {
+            tracing::debug!(%erro, "a discagem de quem empresta não fechou, como se espera")
+        }
+    }
+    std::future::pending().await
+}
+
 /// Os endereços de rede local desta máquina, na porta que `ponta` já usa.
 ///
 /// **Achado do fix round 2.** Enumeração simples, sem a ordenação por
 /// heurística de VPN que `seele-server::alcance::interfaces::descobrir` faz
 /// para o convite: aqui não há convite nem degrau de furo a preparar, só uma
 /// lista de candidatos que `par::ligar` já tenta todos em paralelo. O ADR
 /// 0002 proíbe este crate de depender de `seele-server`, então a pergunta —
 /// "quais endereços desta máquina servem para alguém bater neles" — é
 /// refeita aqui, com o mesmo crate (`if_addrs`).
 ///
 /// Vazio se a enumeração falhar ou não achar nenhum endereço utilizável: o
 /// público que o servidor já vê na conexão de controle continua sobrando
 /// como candidato, e a ausência de locais não impede o furo, só tira o atalho
 /// de LAN.
+///
+/// # Sem caminho de produção hoje
+///
+/// **Nada em `apps/` nem no `seele-ffi` liga o empréstimo**, e esta função só
+/// roda quando alguém o liga (ver [`locais_a_publicar`]). O único chamador do
+/// caminho inteiro no repositório é o teste de integração
+/// `seele-conformance/tests/tela_por_um_par.rs`, que fala com um servidor em
+/// memória — então **o atalho de LAN nunca foi exercitado contra uma rede de
+/// verdade**. `docs/teste-duas-maquinas.md` diz o mesmo, e este parágrafo
+/// existe para que a próxima pessoa não conclua o contrário lendo só o código.
 fn locais_de_pares(ponta: &quinn::Endpoint) -> Vec<SocketAddr> {
     let Ok(local) = ponta.local_addr() else {
         return Vec::new();
     };
     match if_addrs::get_if_addrs() {
         Ok(interfaces) => interfaces
             .into_iter()
             .map(|interface| interface.addr.ip())
             .filter(|ip| e_endereco_de_rede_local(*ip))
             .map(|ip| SocketAddr::new(ip, local.port()))
@@ -3132,20 +3183,29 @@ fn locais_de_pares(ponta: &quinn::Endpoint) -> Vec<SocketAddr> {
 /// máquina: o §5 da spec nomeia privacidade como a primeira das duas razões
 /// independentes do opt-in, e lembra que um servidor não é necessariamente
 /// entre amigos (ADR 0021). Quem só assiste declara a impressão e nada mais —
 /// o público que o servidor já vê na conexão de controle basta para quem
 /// empresta discar de volta, e o furo simultâneo cobre o resto.
 ///
 /// `todos` é adiado (`FnOnce`) de propósito: enumerar as interfaces desta
 /// máquina é trabalho que quem não empresta nem chega a fazer, e um argumento
 /// já avaliado esconderia dentro do chamador justamente a decisão que este
 /// guarda existe para prender.
+///
+/// # O ramo `true` não tem caminho de produção hoje
+///
+/// **Nada em `apps/` nem no `seele-ffi` chama `Enlace::emprestar_subida`**, e
+/// sem isso `emprestando` é sempre `false` em produção: o ramo que publica
+/// endereços só roda no teste de integração
+/// `seele-conformance/tests/tela_por_um_par.rs`. O guarda do opt-in está preso
+/// por teste; o que não foi exercitado é o **caminho de LAN** que ele
+/// destranca. Ver `docs/teste-duas-maquinas.md`, que registra o mesmo.
 fn locais_a_publicar<F: FnOnce() -> Vec<SocketAddr>>(
     emprestando: bool,
     todos: F,
 ) -> Vec<SocketAddr> {
     if emprestando {
         todos()
     } else {
         Vec::new()
     }
 }
@@ -3168,22 +3228,32 @@ fn e_endereco_de_rede_local(ip: IpAddr) -> bool {
     }
 }
 
 /// Quantos pedaços de tela ficam à espera de sair para o par.
 ///
 /// **Um número, e a razão de ele existir é o que se faz quando ele estoura.**
 /// Quem repassa está assistindo à mesma tela, e a leitura dele não pode
 /// esperar a escrita para o par: um par que parou de ler prenderia a imagem de
 /// quem empresta, que é o oposto de «a malha é alívio». Cheio, o repasse é
 /// **desligado inteiro** — nunca é descartado um pedaço no meio, porque um
-/// buraco no fluxo desloca o enquadramento de quem recebe para sempre. Quem
-/// assiste nota a falta de imagem e cai para o servidor pelo caminho de sempre.
+/// buraco no fluxo desloca o enquadramento de quem recebe para sempre.
+///
+/// # Por qual mecanismo quem assiste volta ao servidor
+///
+/// Desligado o destino, o `Sender` cai, [`crate::par::repassar`] termina o
+/// fluxo do par **direito** — e um fluxo que termina direito não é um erro do
+/// outro lado. Por isso o fim limpo de um fluxo de par é reportado como
+/// `ClientMessage::ParFalhou { motivo: ParouDeMandar }` em
+/// [`escoar_tela_alheia`], e não só o fim torto: é esse relato que faz o
+/// servidor religar o cano de quem assiste. Sem ele, esta constante estourar
+/// seria tela em branco permanente — quem assiste já saiu do cano do servidor
+/// desde que o par foi apontado, e nada o recolocaria lá.
 ///
 /// Trinta e dois pedaços são cerca de um segundo a trinta quadros por segundo,
 /// que é muito mais do que uma escrita para um par saudável leva e pouco o
 /// bastante para a memória não crescer sem limite atrás de um par doente.
 const PEDACOS_A_ESPERA_DO_PAR: usize = 32;
 
 /// A tela que chega do servidor, aberta para quem a repassa a um par.
 ///
 /// # Por que um lugar partilhado, e não um argumento
 ///
@@ -3202,84 +3272,162 @@ const PEDACOS_A_ESPERA_DO_PAR: usize = 32;
 /// cabeçalho, que [`crate::par::repassar`] escreve tal e qual —, e o
 /// **destino** dos pedaços, quando há um par sendo servido. Sem abertura não
 /// há o que repassar: um par ligado no meio de uma transmissão cujo cabeçalho
 /// ele nunca viu não decodifica nada.
 ///
 /// `std::sync::Mutex` e não o do `tokio`: nada aqui dentro espera por nada, e
 /// os dois punhos são segurados por microssegundos. Um mutex assíncrono aqui
 /// só acrescentaria pontos de suspensão a caminhos que não os têm.
 #[derive(Debug, Default)]
 struct RepasseDeTela {
+    /// Tudo sob um punho só. Ver o doc de [`EstadoDoRepasse`].
+    estado: std::sync::Mutex<EstadoDoRepasse>,
+}
+
+/// O que o repasse guarda, e por que num punho só.
+///
+/// **Porque as três coisas se decidem juntas.** «Estes bytes vão para o par?»
+/// é uma pergunta sobre a tela em curso *e* sobre haver destino; separada em
+/// dois punhos, ela é respondida em dois instantes, e entre os dois a
+/// transmissão pode ter trocado. Um quadro copiado para o fluxo de outra tela
+/// não dá erro em lugar nenhum — dá duas telas fundidas numa só na janela de
+/// quem assiste, que é o defeito que este guarda existe para impedir.
+#[derive(Debug, Default)]
+struct EstadoDoRepasse {
+    /// De qual transmissão é o repasse em curso.
+    ///
+    /// **É a identidade que faltava, e ela é uma só de propósito.** Havia um
+    /// [`RepasseDeTela`] por [`Motor`] e nenhuma marca de tela: com duas
+    /// transmissões no ar na mesma sala — o cenário do §0 do desenho —, a
+    /// segunda `abriu()` sobrescrevia a primeira, os quadros das duas entravam
+    /// intercalados no mesmo fluxo do par, e o primeiro `fechou()` matava o
+    /// repasse da outra. Quem recebia rotulava tudo com o `screen` do fluxo e
+    /// via as duas telas fundidas, sem um erro em lugar nenhum.
+    ///
+    /// **A versão que repassa as duas é do subprojeto B**, e não cabe aqui:
+    /// ela exige uma marca de tela no fio entre pares — mudança de protocolo,
+    /// que é a fundação daquele subprojeto. O que cabe hoje é a honestidade:
+    /// uma tela por vez, dito em voz alta, com quem assiste a outra
+    /// continuando a receber do servidor pelo caminho de sempre.
+    ///
+    /// A identidade é o [`ScreenId`] e **não** o dono da transmissão: este
+    /// lado não sabe de quem é uma tela alheia. `ScreenHeader` não carrega
+    /// pessoa e [`Aviso::TelaAbriu`] também não — e não precisa carregar: o
+    /// `ScreenId` é atribuído pelo servidor e é único por transmissão, que é
+    /// exatamente a pergunta que este campo responde.
+    qual: Option<ScreenId>,
     /// Os bytes crus do cabeçalho da transmissão que chega agora do servidor.
-    abertura: std::sync::Mutex<Option<Vec<u8>>>,
+    abertura: Option<Vec<u8>>,
     /// Para onde copiar cada quadro, enquanto há um par a servir.
-    destino: std::sync::Mutex<Option<mpsc::Sender<Vec<u8>>>>,
+    destino: Option<mpsc::Sender<Vec<u8>>>,
 }
 
 impl RepasseDeTela {
     /// Uma transmissão do servidor abriu com este cabeçalho.
-    fn abriu(&self, abertura: Vec<u8>) {
-        if let Ok(mut guarda) = self.abertura.lock() {
-            *guarda = Some(abertura);
+    ///
+    /// **A segunda tela não assume.** Se já há um repasse em curso de outra
+    /// transmissão, esta é recusada e o `warn!` diz por quê — ver o doc de
+    /// [`EstadoDoRepasse::qual`]. Quem assiste à tela recusada continua
+    /// recebendo do servidor, que é o caminho de sempre: nenhuma imagem se
+    /// perde, e nada se funde em silêncio.
+    fn abriu(&self, tela: ScreenId, abertura: Vec<u8>) {
+        let Ok(mut estado) = self.estado.lock() else {
+            return;
+        };
+        if let Some(em_curso) = estado.qual {
+            if em_curso != tela {
+                tracing::warn!(
+                    %em_curso,
+                    nova = %tela,
+                    "só uma tela por vez é repassada a um par nesta versão; esta segunda \
+                     transmissão continua vindo do servidor para quem a assiste"
+                );
+                return;
+            }
         }
+        estado.qual = Some(tela);
+        estado.abertura = Some(abertura);
     }
 
     /// A transmissão do servidor acabou: não há mais o que repassar, e o par
     /// que estava sendo servido vê o fluxo terminar direito.
-    fn fechou(&self) {
-        if let Ok(mut guarda) = self.abertura.lock() {
-            *guarda = None;
+    ///
+    /// **Só se for a que está sendo repassada.** Sem esta conferência, o fim
+    /// de uma segunda transmissão — que nunca chegou a ser repassada —
+    /// derrubaria o repasse da primeira.
+    fn fechou(&self, tela: ScreenId) {
+        let Ok(mut estado) = self.estado.lock() else {
+            return;
+        };
+        if estado.qual != Some(tela) {
+            return;
         }
-        self.desligar();
+        estado.qual = None;
+        estado.abertura = None;
+        estado.destino = None;
     }
 
-    /// O cabeçalho da transmissão que está chegando, se há uma.
-    fn abertura(&self) -> Option<Vec<u8>> {
-        self.abertura.lock().ok().and_then(|guarda| guarda.clone())
+    /// O cabeçalho da transmissão que está chegando, se a que chega é esta.
+    ///
+    /// `None` quando o repasse em curso é de outra tela: servir o pedido do
+    /// servidor com a abertura da tela errada seria entregar ao par uma
+    /// transmissão com o nome de outra.
+    fn abertura_de(&self, tela: ScreenId) -> Option<Vec<u8>> {
+        let estado = self.estado.lock().ok()?;
+        if estado.qual != Some(tela) {
+            return None;
+        }
+        estado.abertura.clone()
     }
 
     /// Passa a copiar os pedaços para aqui.
     fn ligar(&self, destino: mpsc::Sender<Vec<u8>>) {
-        if let Ok(mut guarda) = self.destino.lock() {
-            *guarda = Some(destino);
+        if let Ok(mut estado) = self.estado.lock() {
+            estado.destino = Some(destino);
         }
     }
 
     /// Para de copiar. O `Sender` largado fecha o canal, e
     /// [`crate::par::repassar`] termina o fluxo do par direito.
     fn desligar(&self) {
-        if let Ok(mut guarda) = self.destino.lock() {
-            *guarda = None;
+        if let Ok(mut estado) = self.estado.lock() {
+            estado.destino = None;
         }
     }
 
     /// Copia mais um quadro para o par, se há um sendo servido.
     ///
     /// **Nunca espera, e nunca descarta um pedaço só.** Ver o doc de
     /// [`PEDACOS_A_ESPERA_DO_PAR`].
-    fn pedaco(&self, bytes: Vec<u8>) {
-        let Ok(mut guarda) = self.destino.lock() else {
+    ///
+    /// **E só os da tela em curso.** Um quadro de outra transmissão entrando
+    /// neste fluxo é o que fundia duas telas numa só.
+    fn pedaco(&self, tela: ScreenId, bytes: Vec<u8>) {
+        let Ok(mut estado) = self.estado.lock() else {
             return;
         };
-        let Some(destino) = guarda.as_ref() else {
+        if estado.qual != Some(tela) {
+            return;
+        }
+        let Some(destino) = estado.destino.as_ref() else {
             return;
         };
         match destino.try_send(bytes) {
             Ok(()) => {}
             Err(mpsc::error::TrySendError::Full(_)) => {
                 tracing::warn!(
                     quantos = PEDACOS_A_ESPERA_DO_PAR,
                     "o par parou de aceitar bytes; o repasse para ele é desligado"
                 );
-                *guarda = None;
+                estado.destino = None;
             }
-            Err(mpsc::error::TrySendError::Closed(_)) => *guarda = None,
+            Err(mpsc::error::TrySendError::Closed(_)) => estado.destino = None,
         }
     }
 }
 
 /// De onde uma transmissão de tela alheia está chegando.
 ///
 /// **As duas pontas leem o mesmo formato e fazem coisas diferentes com o fim
 /// dele**, e é isso que este `enum` carrega. Do servidor, o fim é a
 /// transmissão acabando, e os pedaços do meio podem interessar a um par que
 /// esta máquina sirva. De um par, o fim no meio é o par tendo caído — e quem
@@ -3380,21 +3528,21 @@ fn escoar_tela_alheia(
                             screen: *screen,
                             motivo: MotivoDeFalhaDePar::CaiuNoMeio,
                         });
                     }
                     return;
                 }
             };
             let cabecalho = *recepcao.cabecalho();
             let tela = cabecalho.screen;
             if let DeOndeVeioATela::Servidor(repasse) = &de_onde {
-                repasse.abriu(recepcao.abertura().to_vec());
+                repasse.abriu(tela, recepcao.abertura().to_vec());
             }
             if avisos
                 .send(Aviso::TelaAbriu {
                     tela,
                     largura: cabecalho.width,
                     altura: cabecalho.height,
                 })
                 .is_err()
             {
                 return;
@@ -3404,21 +3552,21 @@ fn escoar_tela_alheia(
                 match recepcao.proximo_quadro().await {
                     Ok(Some(quadro)) => {
                         // **A cópia para o par sai daqui, antes de qualquer
                         // decisão sobre o que a casca desenha.** Os bytes
                         // repassados são os mesmos que chegaram, na mesma
                         // ordem, som incluído: quem recebe do par tem de ver a
                         // mesma transmissão que quem recebe do servidor, e
                         // filtrar aqui produziria duas telas diferentes com o
                         // mesmo nome.
                         if let DeOndeVeioATela::Servidor(repasse) = &de_onde {
-                            repasse.pedaco(quadro.no_fio());
+                            repasse.pedaco(tela, quadro.no_fio());
                         }
                         // **O som não atravessa a ponte.** Ele vai para a
                         // mistura, aqui em Rust, e nunca para a casca: a janela
                         // não tem o que fazer com um pacote Opus, e mandá-la
                         // decodificar seria dar a ela um trabalho que este lado
                         // já sabe fazer — e que precisa acontecer no mesmo lugar
                         // onde o isolamento total vale.
                         let aviso = if quadro.tipo == crate::tela::TipoDeQuadro::Som {
                             Aviso::TelaSom {
                                 tela,
@@ -3428,21 +3576,50 @@ fn escoar_tela_alheia(
                             Aviso::TelaQuadro {
                                 tela,
                                 chave: quadro.chave(),
                                 bytes: quadro.bytes,
                             }
                         };
                         if avisos.send(aviso).is_err() {
                             return;
                         }
                     }
-                    Ok(None) => break,
+                    Ok(None) => {
+                        // **O fim limpo também é um `ParFalhou`.** Quem
+                        // assiste não tem como distinguir «a transmissão
+                        // acabou» de «o par calou»: as duas chegam como um
+                        // fluxo que termina sem erro. E há três caminhos que
+                        // terminam limpo com a transmissão ainda no ar — a
+                        // contrapressão de [`PEDACOS_A_ESPERA_DO_PAR`], quem
+                        // empresta reconectando ao servidor, e quem empresta
+                        // saindo da sala. Calar aqui é tela em branco
+                        // permanente, porque o cano do servidor para esta
+                        // pessoa foi desligado quando o par foi apontado.
+                        //
+                        // Então reporta sempre, e deixa o **servidor**
+                        // adjudicar: ele é o único que sabe se a transmissão
+                        // ainda existe, e o braço de `ParFalhou` dele já não
+                        // faz nada quando ela acabou de verdade.
+                        //
+                        // `ParouDeMandar` e não `CaiuNoMeio`: nada caiu. O par
+                        // fechou o fluxo direito e simplesmente não manda
+                        // mais. Nenhum dos dois desacredita ninguém
+                        // (`crate::pares::quem_desacreditar`, no servidor), e
+                        // o motivo é o que fica no rastro.
+                        if let DeOndeVeioATela::Par { screen, resultados } = &de_onde {
+                            let _ = resultados.send(ResultadoDoPar::ParFalhou {
+                                screen: *screen,
+                                motivo: MotivoDeFalhaDePar::ParouDeMandar,
+                            });
+                        }
+                        break;
+                    }
                     Err(erro) => {
                         // Um quadro torto encerra esta transmissão e não a
                         // conexão: o fluxo já perdeu o sincronismo, e continuar
                         // lendo dele é ler lixo. Quem transmite recomeça com um
                         // fluxo novo se quiser.
                         // `warn!`: o fluxo perdeu o sincronismo no meio, e do
                         // lado de quem assiste isso é a imagem congelando. O
                         // `TelaFechou` logo abaixo ao menos apaga o palco, que é
                         // mais do que o caso do cabeçalho tinha.
                         tracing::warn!(%erro, %tela, "a transmissão alheia terminou torta");
@@ -3456,21 +3633,21 @@ fn escoar_tela_alheia(
                             let _ = resultados.send(ResultadoDoPar::ParFalhou {
                                 screen: *screen,
                                 motivo: MotivoDeFalhaDePar::CaiuNoMeio,
                             });
                         }
                         break;
                     }
                 }
             }
             if let DeOndeVeioATela::Servidor(repasse) = &de_onde {
-                repasse.fechou();
+                repasse.fechou(tela);
             }
             let _ = avisos.send(Aviso::TelaFechou { tela });
         });
     }
 }
 
 impl Motor {
     fn e_a_minha_tela(&self, tela: ScreenId) -> bool {
         self.tela_viva
             .as_ref()
@@ -5124,11 +5301,123 @@ mod tests {
             Some(&identidade_assiste),
             Duration::from_millis(300),
         )
         .await;
         assert!(
             matches!(tentou, Err(par::ErroDePar::NaoAlcancou)),
             "a ponta de quem empresta continuou atendendo depois de servir_um_par devolver: \
              {tentou:?}"
         );
     }
+
+    /// **Duas telas na mesma sala: a segunda não é repassada, e a primeira
+    /// sobrevive ao fim dela.**
+    ///
+    /// O cenário é o do §0 do desenho — «numa call de 5 pessoas, 2 querem
+    /// transmitir a tela» —, e antes deste guarda ele produzia o pior defeito
+    /// que este repositório sabe nomear: a segunda `abriu()` sobrescrevia a
+    /// primeira, os quadros das duas entravam intercalados no mesmo fluxo do
+    /// par, quem recebia rotulava tudo com o `screen` do fluxo, e o primeiro
+    /// `fechou()` matava o repasse da outra. Duas telas fundidas numa só, sem
+    /// erro em lugar nenhum.
+    #[tokio::test]
+    async fn so_uma_tela_por_vez_e_repassada_e_o_fim_da_outra_nao_a_derruba() {
+        let repasse = RepasseDeTela::default();
+        let primeira = ScreenId(1);
+        let segunda = ScreenId(2);
+        let (para_o_par, mut do_par) = mpsc::channel(8);
+
+        repasse.abriu(primeira, b"abertura-da-primeira".to_vec());
+        repasse.ligar(para_o_par);
+
+        // A segunda transmissão chega e **não** assume.
+        repasse.abriu(segunda, b"abertura-da-segunda".to_vec());
+        assert_eq!(
+            repasse.abertura_de(primeira),
+            Some(b"abertura-da-primeira".to_vec()),
+            "a segunda tela assumiu o repasse da primeira"
+        );
+        assert_eq!(
+            repasse.abertura_de(segunda),
+            None,
+            "o repasse aceitou servir a segunda tela com a abertura de outra"
+        );
+
+        // E os quadros dela não entram no fluxo do par.
+        repasse.pedaco(segunda, b"quadro-da-segunda".to_vec());
+
+        // O fim da segunda não derruba o repasse da primeira.
+        repasse.fechou(segunda);
+        repasse.pedaco(primeira, b"quadro-da-primeira".to_vec());
+
+        assert_eq!(
+            do_par.try_recv().ok(),
+            Some(b"quadro-da-primeira".to_vec()),
+            "o fim da segunda tela derrubou o repasse da primeira"
+        );
+        assert!(
+            do_par.try_recv().is_err(),
+            "um quadro da segunda tela entrou no fluxo do par da primeira — as duas chegam \
+             fundidas a quem assiste"
+        );
+
+        // E o fim da primeira, esse sim, encerra o repasse.
+        repasse.fechou(primeira);
+        assert_eq!(repasse.abertura_de(primeira), None);
+    }
+
+    /// **A discagem de quem empresta falha na hora, e `atender` ainda serve.**
+    ///
+    /// Quem assiste nunca chama `par::atender` — só disca. Então a discagem de
+    /// quem empresta não tem quem a atenda e **não pode** fechar: ela existe
+    /// por um efeito só, abrir o mapeamento de NAT desta ponta. O `select!`
+    /// tratava o fim dela como resposta, e bastava um erro rápido — lista de
+    /// candidatos vazia, família de endereço incompatível, `connect_with`
+    /// recusando na hora — para `atender` ser cancelado e quem empresta
+    /// desistir de servir alguém que estava no meio do caminho.
+    ///
+    /// A lista vazia é o erro instantâneo mais limpo que existe: `par::ligar`
+    /// não tem candidato para tentar e devolve `NaoAlcancou` sem esperar um
+    /// milissegundo.
+    #[tokio::test(flavor = "multi_thread")]
+    async fn a_discagem_que_falha_na_hora_nao_cancela_o_atendimento() {
+        let ponta_empresta = quinn::Endpoint::client("127.0.0.1:0".parse().unwrap()).unwrap();
+        let onde_empresta = ponta_empresta.local_addr().unwrap();
+        let identidade_empresta = par::identidade_efemera().unwrap();
+        let impressao_empresta = par::impressao(&identidade_empresta);
+
+        let identidade_assiste = par::identidade_efemera().unwrap();
+        let impressao_assiste = par::impressao(&identidade_assiste);
+
+        // **Sem endereço nenhum para discar.** `par::ligar` desiste no mesmo
+        // instante, e é esse instante que cancelava o `atender`.
+        let servindo = tokio::spawn(servir_um_par(
+            ponta_empresta.clone(),
+            identidade_empresta,
+            Vec::new(),
+            impressao_assiste.clone(),
+        ));
+
+        // E quem assiste chega, como sempre chega: discando.
+        let ponta_assiste = quinn::Endpoint::client("127.0.0.1:0".parse().unwrap()).unwrap();
+        let ligado = par::ligar(
+            &ponta_assiste,
+            &[onde_empresta],
+            impressao_empresta,
+            Some(&identidade_assiste),
+            Duration::from_secs(3),
+        )
+        .await;
+
+        let resultado = servindo.await.expect("a tarefa de servir_um_par");
+        assert!(
+            resultado.is_some(),
+            "a discagem de quem empresta falhou na hora e levou o `atender` junto: quem \
+             empresta desistiu de servir alguém que estava chegando"
+        );
+        assert!(
+            ligado.is_ok(),
+            "quem assiste discou para uma ponta que devia estar atendendo e não fechou: {:?}",
+            ligado.err()
+        );
+    }
 }
diff --git a/crates/seele-core/src/par.rs b/crates/seele-core/src/par.rs
index 6a7dd44..cc48f88 100644
--- a/crates/seele-core/src/par.rs
+++ b/crates/seele-core/src/par.rs
@@ -555,23 +555,40 @@ pub struct ParLigado {
 /// pior caso é justamente o endereço que não responde.
 ///
 /// **Só disca.** Quem atende do outro lado é o laço de [`quinn::Endpoint::accept`]
 /// de quem chamou [`passar_a_atender`] — o `quinn` enfileira a tentativa que
 /// chega e não responde nada até alguém a aceitar. As duas coisas juntas é que
 /// são o furo: a discagem abre o mapeamento de NAT desta ponta, e o atendimento
 /// deixa entrar a discagem que vem pelo mapeamento aberto do outro lado.
 ///
 /// # Errors
 ///
-/// [`ErroDePar::ImpressaoNaoBate`] quando alguém respondeu e não era quem o
-/// servidor apresentou; [`ErroDePar::NaoAlcancou`] quando ninguém respondeu no
-/// prazo.
+/// Cinco motivos, e a diferença entre eles é a diferença entre cinco consertos
+/// — numa casa em que o motivo enumerado é contrato de casca (ADR 0012), uma
+/// lista incompleta aqui é a casca escolhendo a frase errada:
+///
+/// - [`ErroDePar::NaoAlcancou`] — nenhum dos endereços respondeu dentro do
+///   prazo. É o silêncio: rede, NAT, endereço velho.
+/// - [`ErroDePar::ImpressaoNaoBate`] — alguém respondeu e não era quem o
+///   servidor apresentou. Único motivo que é evento de segurança, e o único
+///   que desacredita a declaração de quem foi apontado.
+/// - [`ErroDePar::RecusadoDepoisDeLigar`] — o aperto de mão fechou deste lado
+///   e quem atendeu recusou a contrapartida. Leva o [`MotivoDaRecusa`] dentro.
+/// - [`ErroDePar::ConfirmacaoNaoChegouATempo`] — alguém completou o TLS e o
+///   prazo venceu antes da troca do byte de confirmação. Não é
+///   `NaoAlcancou`: houve resposta.
+/// - [`ErroDePar::Escuta`] — a configuração de cliente desta discagem não
+///   pôde ser montada, `connect_with` recusou o endereço na hora, ou uma
+///   tentativa morreu sem responder por si.
+///
+/// [`ErroDePar::Certificado`] e [`ErroDePar::Repasse`] **não** saem daqui: o
+/// primeiro é de [`passar_a_atender`], o segundo de [`repassar`].
 pub async fn ligar(
     ponta: &quinn::Endpoint,
     enderecos: &[std::net::SocketAddr],
     impressao_esperada: String,
     identidade_propria: Option<&Identidade>,
     prazo: std::time::Duration,
 ) -> Result<ParLigado, ErroDePar> {
     let mut tentativas = tokio::task::JoinSet::new();
     // **Marca se alguma tentativa chegou a completar o TLS.** Uma tentativa
     // presa em `confirmar_com_quem_atende` quando o prazo vence é abortada
diff --git a/crates/seele-proto/src/control.rs b/crates/seele-proto/src/control.rs
index a22b559..dacfdbf 100644
--- a/crates/seele-proto/src/control.rs
+++ b/crates/seele-proto/src/control.rs
@@ -220,29 +220,29 @@ pub const PUBLIC_KEY_LEN: usize = 32;
 
 /// Length of an Ed25519 signature, in bytes.
 pub const SIGNATURE_LEN: usize = 64;
 
 /// Longest authentication proof, in bytes.
 ///
 /// An Ed25519 signature is 64 bytes (ADR 0004); the slack is for whatever a
 /// password fallback needs.
 pub const MAX_PROOF_LEN: usize = 256;
 
-/// Tamanho de uma impressão digital em hex minúsculo.
+/// Tamanho de uma impressão digital em hex.
 ///
 /// Um SHA-256 em hexadecimal são exatamente 64 caracteres — nem mais, nem
 /// menos, o mesmo tamanho que [`crate::uri`] já confere para o `fp=` do
-/// `seele://`. Só o tamanho é conferido aqui: se os caracteres são hex e se a
-/// impressão bate com um certificado de verdade é o que o aperto de mão
-/// descobre, e é para isso que [`MotivoDeFalhaDePar::ImpressaoNaoBate`]
-/// existe.
-pub const MAX_IMPRESSAO_LEN: usize = 64;
+/// `seele://`. É um tamanho **exato**, e não um teto: ver
+/// [`check_impressao`]. Se a impressão bate com um certificado de verdade é o
+/// que o aperto de mão descobre, e é para isso que
+/// [`MotivoDeFalhaDePar::ImpressaoNaoBate`] existe.
+pub const IMPRESSAO_LEN: usize = 64;
 
 /// Why a control frame could not be handled.
 #[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
 pub enum ControlError {
     /// An empty frame carries not even a version.
     #[error("control frame is empty")]
     Empty,
 
     /// Longer than [`MAX_FRAME_LEN`].
     #[error("control frame is {len} bytes, over the {MAX_FRAME_LEN}-byte limit")]
@@ -1946,20 +1946,43 @@ pub trait Validate {
     /// # Errors
     ///
     /// Returns [`ControlError::FieldTooLong`] for the first field over its limit.
     fn validate(&self) -> Result<(), ControlError>;
 }
 
 fn check(field: &'static str, len: usize, limit: usize) -> Result<(), ControlError> {
     check_bounds(field, len, limit)
 }
 
+/// Recusa uma impressão digital que não é um SHA-256 em hexadecimal.
+///
+/// **Tamanho exato, e não teto.** Um `<= 64` aceita `""`, aceita um caractere,
+/// e aceita sessenta e quatro caracteres que não são hexadecimais — e um
+/// cliente que declare lixo é escolhido por `Pares::escolher`, ocupa vaga em
+/// `ja_servindo`, e custa segundos de tela parada a cada `WatchScreen`: a
+/// discagem contra uma impressão que nenhum certificado produz só falha quando
+/// o prazo vence. Falhar aqui custa um quadro de controle.
+///
+/// A mesma regra que [`crate::uri`] aplica ao `fp=` do `seele://` — um formato
+/// só para a mesma coisa —, maiúsculas incluídas.
+///
+/// # Errors
+///
+/// [`ControlError::FieldOutOfRange`] quando não são exatamente
+/// [`IMPRESSAO_LEN`] dígitos hexadecimais.
+fn check_impressao(impressao: &str) -> Result<(), ControlError> {
+    if impressao.len() == IMPRESSAO_LEN && impressao.chars().all(|c| c.is_ascii_hexdigit()) {
+        return Ok(());
+    }
+    Err(ControlError::FieldOutOfRange { field: "impressao" })
+}
+
 /// Refuses a field longer than its documented limit.
 ///
 /// Public because [`crate::attachment`] validates the header of a transfer with
 /// the same rules, on a different stream. One implementation rather than two
 /// that drift.
 ///
 /// # Errors
 ///
 /// Returns [`ControlError::FieldTooLong`] when `len` exceeds `limit`.
 pub fn check_bounds(field: &'static str, len: usize, limit: usize) -> Result<(), ControlError> {
@@ -2179,23 +2202,21 @@ impl Validate for ClientMessage {
             | Self::FetchAttachment { .. }
             | Self::StartScreenShare
             | Self::StopScreenShare
             | Self::RequestKeyFrame { .. }
             // A `ScreenId` não se valida aqui: um número qualquer é uma tela que
             // não existe, e quem sabe disso é o servidor, que tem o registro. O
             // que ele faz com um id inventado está em `session.rs`, junto da
             // mesma conferência que o pedido de quadro-chave já faz.
             | Self::WatchScreen { .. }
             | Self::UnwatchScreen { .. } => Ok(()),
-            Self::EmprestarSubida { impressao, .. } => {
-                check("impressao", impressao.len(), MAX_IMPRESSAO_LEN)
-            }
+            Self::EmprestarSubida { impressao, .. } => check_impressao(impressao),
             // O motivo é um enumerado de tamanho fixo, e a `ScreenId` segue a
             // mesma regra do braço acima: quem sabe se ela existe é o servidor.
             Self::ParFalhou { .. } => Ok(()),
         }
     }
 }
 
 impl Validate for ServerMessage {
     fn validate(&self) -> Result<(), ControlError> {
         match self {
@@ -2282,22 +2303,26 @@ impl Validate for ServerMessage {
             | Self::VoiceRoomDeleted { .. }
             | Self::ChannelDeleted { .. }
             | Self::ChannelWeighed { .. }
             | Self::AttachmentRefused { .. }
             | Self::AttachmentUnavailable { .. }
             | Self::ScreenShareStarted { .. }
             | Self::ScreenShareStopped { .. }
             | Self::KeyFrameRequested { .. }
             | Self::ScreenViewers { .. }
             | Self::HostUplink { .. } => Ok(()),
+            // A mesma regra da entrada, e não uma mais frouxa: a impressão que
+            // sai daqui veio de uma declaração que já passou por
+            // `check_impressao`, e uma saída que aceitasse o que a entrada
+            // recusa seria uma segunda regra esperando para discordar.
             Self::SirvaTelaPara { impressao, .. } | Self::AssistaTelaPor { impressao, .. } => {
-                check("impressao", impressao.len(), MAX_IMPRESSAO_LEN)
+                check_impressao(impressao)
             }
         }
     }
 }
 
 #[cfg(test)]
 mod tests {
     use super::*;
     use proptest::prelude::*;
 
@@ -3665,21 +3690,25 @@ mod o_vocabulario_e_a_versao {
             }),
             33,
             "a lista do cliente mudou de tamanho. Leia o doc deste teste antes \
              de mexer no número: a pergunta é sobre `PROTOCOL_VERSION`, e não \
              sobre esta linha"
         );
         assert_eq!(
             ordinal(&ServerMessage::AssistaTelaPor {
                 screen: ScreenId(1),
                 enderecos: vec![],
-                impressao: String::new(),
+                // Uma impressão de verdade, e não `String::new()`: `encode`
+                // valida antes de serializar, e `check_impressao` exige um
+                // SHA-256 em hex. A pergunta deste teste é sobre o ordinal, e
+                // um campo inválido a trocaria por outra.
+                impressao: "3c".repeat(32),
             }),
             35,
             "a lista do servidor mudou de tamanho. Leia o doc deste teste antes \
              de mexer no número"
         );
 
         // E o número da versão, preso ao lado deles. Ele é o que diz a um par se
         // vale a pena tentar — e enquanto ele não subir, dois builds com listas
         // diferentes vão continuar se cumprimentando como iguais.
         // **Este número já cumpriu o trabalho dele duas vezes.** Ele estava em 2
@@ -3690,11 +3719,80 @@ mod o_vocabulario_e_a_versao {
         // pares: `EmprestarSubida`, `ParFalhou`, `SirvaTelaPara` e
         // `AssistaTelaPor` entraram, os dois braços acima passaram a apontar
         // para o verbo novo de cada lista, e a versão subiu para 4.
         assert_eq!(
             crate::version::PROTOCOL_VERSION,
             4,
             "a versão do protocolo mudou; confira se os ordinais acima e a janela \
              de compatibilidade continuam contando a mesma história"
         );
     }
+
+    #[test]
+    fn uma_impressao_que_nao_e_um_sha256_em_hex_e_recusada_no_fio() {
+        // **O teto não bastava.** `check(.., MAX_IMPRESSAO_LEN)` aceitava
+        // `""`, aceitava um caractere, e aceitava sessenta e quatro caracteres
+        // que não são hexadecimais. O desenho diz «exatamente 64 caracteres», e
+        // a diferença não é cosmética: um cliente que declare lixo é escolhido
+        // por `Pares::escolher`, ocupa vaga em `ja_servindo`, e custa segundos
+        // de tela parada a cada `WatchScreen` — porque a discagem contra uma
+        // impressão que nenhum certificado produz só falha quando o prazo
+        // vence.
+        for ruim in [
+            String::new(),
+            "a".repeat(63),
+            "a".repeat(65),
+            "z".repeat(64),
+            format!("{}g", "a".repeat(63)),
+        ] {
+            let quantos = ruim.len();
+            let mensagem = ClientMessage::EmprestarSubida {
+                emprestando: true,
+                impressao: ruim,
+                locais: Vec::new(),
+            };
+            assert!(
+                mensagem.validate().is_err(),
+                "uma impressão de {quantos} caracteres que não é um SHA-256 em hex passou pela \
+                 validação"
+            );
+        }
+    }
+
+    #[test]
+    fn uma_impressao_de_verdade_continua_passando() {
+        // A outra metade: um guarda que recusasse tudo passaria no teste acima
+        // e desligaria a malha inteira. Maiúsculas incluídas, como
+        // `crate::uri` já aceita para o `fp=` do `seele://` — um formato só
+        // para a mesma coisa.
+        for boa in ["3c".repeat(32), "3C".repeat(32)] {
+            let mensagem = ClientMessage::EmprestarSubida {
+                emprestando: true,
+                impressao: boa.clone(),
+                locais: Vec::new(),
+            };
+            assert!(
+                mensagem.validate().is_ok(),
+                "uma impressão legítima ({boa}) foi recusada"
+            );
+        }
+    }
+
+    #[test]
+    fn as_duas_mensagens_do_servidor_conferem_a_impressao_com_a_mesma_regra() {
+        // A impressão que o servidor apresenta veio de uma declaração que já
+        // passou pela regra acima; conferi-la de novo com uma regra **mais
+        // frouxa** seria deixar a saída aceitar o que a entrada recusa.
+        let sirva = ServerMessage::SirvaTelaPara {
+            screen: ScreenId(7),
+            enderecos: Vec::new(),
+            impressao: String::new(),
+        };
+        assert!(sirva.validate().is_err());
+        let assista = ServerMessage::AssistaTelaPor {
+            screen: ScreenId(7),
+            enderecos: Vec::new(),
+            impressao: "z".repeat(64),
+        };
+        assert!(assista.validate().is_err());
+    }
 }
diff --git a/crates/seele-server/src/pares.rs b/crates/seele-server/src/pares.rs
index 9a17b8a..bd6bf09 100644
--- a/crates/seele-server/src/pares.rs
+++ b/crates/seele-server/src/pares.rs
@@ -1,19 +1,20 @@
 //! Quem declarou identidade para o caminho entre pares — e, entre esses,
 //! quem empresta a subida agora.
 //!
 //! # A escolha aqui é deliberadamente burra
 //!
 //! Ela não segue critério visível nenhum — nem latência, nem ordem de
 //! chegada: `self.quem` é um `HashMap`, e a ordem de iteração dele não é a de
-//! inserção. O que ela garante é só isto — não é quem compartilha, e ainda
-//! não serve ninguém. É um espaço reservado com a forma certa: o
+//! inserção. O que ela garante é só isto — não é quem compartilha, ainda não
+//! serve ninguém, e **está na mesma sala de voz da transmissão**. É um espaço
+//! reservado com a forma certa: o
 //! **subprojeto B** é quem olha subida medida e topologia para escolher bem.
 //! Chamar isto de «escolha automática» seria vender como pronto o que é um
 //! lugar guardado — e a spec de 05/09 diz isso com todas as letras.
 //!
 //! # Por que a identidade sobrevive a `emprestando: false`
 //!
 //! Achado do fix round 1 da Task 8, sobre um ruling meu de pré-voo que
 //! misturava duas perguntas diferentes: **quem eu sou** e **eu empresto**. A
 //! parede simétrica da Task 5 exige certificado dos dois lados de toda
 //! ligação entre pares — quem atende confere quem chega contra a impressão
@@ -47,47 +48,89 @@ use seele_proto::ids::{PersonId, ScreenId};
 pub struct QuemDeclarou {
     /// Quem.
     pub pessoa: PersonId,
     /// Qual conexão fez esta declaração — `quinn::Connection::stable_id`,
     /// como `session.rs` já usa para `Subida::esquecer`.
     ///
     /// **Não é decoração.** Uma corrida de candidatos pode ter duas conexões
     /// vivas com o mesmo `PersonId`; sem isto, o encerramento de uma conexão
     /// que perdeu a corrida apagaria a declaração de uma que ganhou. Ver o
     /// doc do módulo.
+    ///
+    /// # A ressalva, e o que quebra se ela cair
+    ///
+    /// `stable_id()` é, no `quinn` 0.11, o **endereço de alocação** do estado
+    /// interno da conexão. Ele é estável enquanto a conexão vive, que é tudo
+    /// o que este campo precisa — mas ele pode ser **reusado** depois que ela
+    /// morre, porque o alocador pode devolver o mesmo endereço a outra
+    /// conexão.
+    ///
+    /// Se essa suposição cair, [`Pares::saiu`] passa a apagar a declaração
+    /// **viva** de outra conexão: o encerramento tardio de uma conexão morta
+    /// chegaria com um `id_da_conexao` que agora pertence a outra pessoa, a
+    /// conferência bateria por engano, e quem acabou de declarar sairia da
+    /// malha sem ter feito nada — caladamente, porque não há erro nenhum a
+    /// dar nesse caminho.
+    ///
+    /// Este campo passou de decoração a **carregar a correção** de uma rodada
+    /// de conserto; a suposição está escrita aqui, onde a invariante mora, e
+    /// não num relatório. O dia em que `stable_id` deixar de servir, é este
+    /// doc que diz o que pôr no lugar: um identificador de sessão que o
+    /// servidor mesmo atribua, e que não venha do alocador de ninguém.
     pub id_da_conexao: u64,
     /// A impressão digital que apresenta.
     pub impressao: String,
     /// Onde alcançá-la: os locais que declarou, mais o público que o servidor
     /// **viu**. Nesta ordem, porque a rede local dispensa furo e é a que
     /// responde mais rápido — a mesma razão do ADR 0037.
     pub enderecos: Vec<SocketAddr>,
     /// Se, além de existir, também empresta a subida agora.
     ///
     /// É só este campo que [`Pares::escolher`] olha. Ter identidade aqui e
     /// `emprestando: false` é o caso comum de quem só assiste — ver o doc do
     /// módulo.
     pub emprestando: bool,
 }
 
-/// Quem declarou identidade para o caminho entre pares nesta sala, agora.
+/// Uma nomeação do servidor: quem empresta, e para quem.
+///
+/// **Os dois lados, e não só quem empresta.** Guardar quem empresta basta para
+/// resolver um `ParFalhou`, mas não para desfazer a nomeação quando quem
+/// assiste vai embora — e uma nomeação que não é desfeita é um par que o
+/// servidor nunca mais escolhe. Ver [`Pares::desapontou`].
+#[derive(Debug, Clone, Copy, PartialEq, Eq)]
+struct Nomeacao {
+    /// Quem foi apontado para servir.
+    empresta: PersonId,
+    /// A quem ele foi mandado servir.
+    assiste: PersonId,
+}
+
+/// Quem declarou identidade para o caminho entre pares neste daemon, agora.
+///
+/// **Global ao daemon, e não por sala de voz.** Uma pessoa declara identidade
+/// uma vez por sessão, e continua a mesma pessoa ao trocar de sala. É por isso
+/// que [`Self::escolher`] recebe de fora quem está na sala da transmissão: a
+/// pergunta «quem é você» mora aqui, e a pergunta «onde você está agora» mora
+/// na `crate::server::Occupancy`, que é reescrita a cada entrada e saída.
 #[derive(Debug, Default)]
 pub struct Pares {
     quem: HashMap<PersonId, QuemDeclarou>,
-    /// Quem o servidor apontou por último para servir cada transmissão.
+    /// Quem o servidor apontou por último para servir cada transmissão, e a
+    /// quem.
     ///
     /// **A própria nomeação do servidor, guardada para poder ser desfeita.**
     /// Achado do fix round 2: sem isto, um `ParFalhou { screen }` não tem
     /// como saber de quem reclamar — ele só carrega a transmissão, e nunca
     /// deveria carregar a identidade de quem falhou, porque quem relata é a
     /// vítima, não quem investiga. Ver [`Self::apontou`].
-    nomeacoes: HashMap<ScreenId, PersonId>,
+    nomeacoes: HashMap<ScreenId, Nomeacao>,
 }
 
 impl Pares {
     /// Ninguém declarado ainda.
     #[must_use]
     pub fn nova() -> Self {
         Self::default()
     }
 
     /// Alguém declarou identidade — e disse se empresta a subida com ela.
@@ -136,119 +179,178 @@ impl Pares {
     /// `id_da_conexao` não bater com o que está publicado, não há nada a
     /// fazer: outra conexão já substituiu esta declaração.
     pub fn saiu(&mut self, pessoa: PersonId, id_da_conexao: u64) {
         let e_esta_conexao = self
             .quem
             .get(&pessoa)
             .is_some_and(|declarado| declarado.id_da_conexao == id_da_conexao);
         if e_esta_conexao {
             self.esquecer(pessoa);
         }
+        // **Fora do `if`, e de propósito.** A conferência por conexão existe
+        // para não apagar a *declaração* de uma conexão que venceu a corrida
+        // de candidatos. Uma nomeação feita para esta pessoa não é declaração
+        // de ninguém: quem assiste foi embora, o repasse acabou, e o par
+        // apontado tem de voltar à fila seja qual for a conexão que fechou.
+        self.quem_assiste_saiu(pessoa);
     }
 
     /// Desacredita a declaração desta pessoa, **seja qual for a conexão que a
     /// fez**.
     ///
     /// Diferente de [`Self::saiu`]: ali o motivo é a conexão ter acabado, e a
     /// checagem por `id_da_conexao` existe para não confundir sessões.
     /// Aqui o motivo é a **declaração** ter sido provada falsa —
     /// `ImpressaoNaoBate`, quando alguém respondeu no lugar de quem foi
     /// apontado — e a conexão que a fez pode continuar perfeitamente viva; é
     /// a identidade publicada que deixou de merecer confiança. Ver
     /// [`quem_desacreditar`].
     pub fn desacreditar(&mut self, pessoa: PersonId) {
         self.esquecer(pessoa);
     }
 
     /// O que [`Self::saiu`] e [`Self::desacreditar`] têm em comum: apagar a
     /// declaração e qualquer nomeação que apontava para ela.
     fn esquecer(&mut self, pessoa: PersonId) {
         self.quem.remove(&pessoa);
-        self.nomeacoes.retain(|_, quem| *quem != pessoa);
+        self.nomeacoes
+            .retain(|_, nomeacao| nomeacao.empresta != pessoa);
+    }
+
+    /// Esta pessoa deixou de assistir a tudo — saiu da sala, ou a sessão dela
+    /// acabou.
+    ///
+    /// **É metade do conserto de «cada repasse queima um par para sempre».**
+    /// A nomeação existe enquanto alguém está sendo servido; quem assiste
+    /// indo embora encerra o repasse tanto quanto um `UnwatchScreen`, e sem
+    /// esta linha o par apontado ficaria contado em [`Self::ja_servindo`] pelo
+    /// resto da sessão do daemon, enquanto o cliente dele já devolveu a vaga.
+    /// Os dois lados discordariam em silêncio, e a malha degradaria para a
+    /// estrela sem um rastro.
+    pub fn quem_assiste_saiu(&mut self, pessoa: PersonId) {
+        self.nomeacoes
+            .retain(|_, nomeacao| nomeacao.assiste != pessoa);
     }
 
     /// A declaração desta pessoa, exista ela para emprestar ou só para ser
     /// alcançada.
     ///
     /// `None` se ela nunca declarou, ou já saiu. Quem vai montar
     /// `SirvaTelaPara`/`AssistaTelaPor` precisa disto para a identidade de
     /// **quem pediu** — `escolher` só devolve a de quem empresta.
     #[must_use]
     pub fn declaracao_de(&self, pessoa: PersonId) -> Option<&QuemDeclarou> {
         self.quem.get(&pessoa)
     }
 
     /// Quem pode servir esta transmissão a esta pessoa, se alguém.
     ///
     /// Só considera quem declarou `emprestando: true` — ter identidade
     /// guardada não é o mesmo que ter optado por emprestar. Ver o doc de
     /// [`QuemDeclarou::emprestando`] e do módulo.
+    ///
+    /// # `na_sala` não é um refinamento: é a parede
+    ///
+    /// Este registro é **global ao daemon**, e não por sala: uma pessoa
+    /// declara identidade uma vez por sessão, não uma vez por sala em que
+    /// senta. Sem `na_sala`, quem empresta na sala B era escolhido para servir
+    /// a tela da sala A — e o cliente dele repassa o que **ele** está
+    /// recebendo, que é a tela da sala B. Quem assiste na sala A recebia,
+    /// decodificava e mostrava conteúdo de uma sala em que nunca entrou.
+    ///
+    /// O §5 do desenho justifica a privacidade do repasse dizendo que «quem
+    /// repassa já é espectador autorizado daquele fluxo». A frase só é
+    /// verdade se quem repassa e quem recebe estiverem na mesma sala, e é
+    /// esta linha que faz disso um fato em vez de uma suposição.
+    ///
+    /// `na_sala` é quem está **agora** na sala de voz da transmissão, e vem de
+    /// fora de propósito: a fonte viva é `crate::server::Occupancy`, que é
+    /// reescrita a cada entrada e saída. Guardar a sala dentro de
+    /// [`QuemDeclarou`] daria um campo escrito uma vez na declaração e nunca
+    /// mais — e pessoas trocam de sala sem redeclarar nada.
     #[must_use]
     pub fn escolher(
         &self,
         dono: PersonId,
         quem_quer: PersonId,
         ja_servindo: &HashSet<PersonId>,
+        na_sala: &HashSet<PersonId>,
     ) -> Option<QuemDeclarou> {
         self.quem
             .values()
             .find(|candidato| {
                 candidato.emprestando
                     && candidato.pessoa != dono
                     && candidato.pessoa != quem_quer
                     && !ja_servindo.contains(&candidato.pessoa)
+                    && na_sala.contains(&candidato.pessoa)
             })
             .cloned()
     }
 
     /// Quem já está apontado para servir alguma transmissão agora.
     ///
     /// É o `ja_servindo` que [`Self::escolher`] recebe. Sai das próprias
     /// nomeações do servidor porque elas são o único registro de quem ele já
     /// pôs para trabalhar — contar de outro lugar seria uma segunda conta a
     /// discordar desta no primeiro dia ruim.
     #[must_use]
     pub fn ja_servindo(&self) -> HashSet<PersonId> {
-        self.nomeacoes.values().copied().collect()
+        self.nomeacoes
+            .values()
+            .map(|nomeacao| nomeacao.empresta)
+            .collect()
     }
 
-    /// O servidor apontou `quem` para servir `screen`.
+    /// O servidor apontou `empresta` para servir `screen` a `assiste`.
     ///
     /// Chamado por quem despacha `SirvaTelaPara`/`AssistaTelaPor`, depois de
     /// [`Self::escolher`] decidir. Substitui a nomeação anterior desta
     /// transmissão, se havia uma: só a mais recente importa para resolver um
     /// `ParFalhou`.
-    pub fn apontou(&mut self, screen: ScreenId, quem: PersonId) {
-        self.nomeacoes.insert(screen, quem);
+    pub fn apontou(&mut self, screen: ScreenId, empresta: PersonId, assiste: PersonId) {
+        self.nomeacoes
+            .insert(screen, Nomeacao { empresta, assiste });
     }
 
     /// Esta transmissão deixou de ter par apontado.
     ///
+    /// **Chamada em todo caminho que encerra o repasse**, e não só no
+    /// `ParFalhou`: quem assiste faz `UnwatchScreen`, quem compartilha para,
+    /// a sala é apagada. Enquanto só o relato de falha a chamava, um repasse
+    /// que terminasse **bem** deixava a nomeação de pé, e
+    /// [`Self::ja_servindo`] contava aquele par como ocupado pelo resto da
+    /// sessão do daemon — com o cliente dele já tendo devolvido a vaga. A
+    /// malha degradava para a estrela, um par por transmissão encerrada, sem
+    /// um único rastro dizendo por quê.
+    ///
     /// **Diferente de [`Self::desacreditar`], e a diferença é quem paga.** Ali
     /// a declaração inteira de uma pessoa é apagada, porque ela foi provada
     /// falsa; aqui só a nomeação some, e quem emprestava continua declarado e
     /// elegível. É o que um `ParFalhou` de rotina — o par caiu, o par parou de
     /// mandar — merece: a transmissão volta ao servidor, e ninguém é punido
     /// por a rede de alguém ter oscilado.
     pub fn desapontou(&mut self, screen: ScreenId) {
         self.nomeacoes.remove(&screen);
     }
 
     /// Quem foi apontado por último para servir esta transmissão, se alguém.
     ///
     /// É contra isto que um `ClientMessage::ParFalhou { screen }` se resolve:
     /// a mensagem só carrega a transmissão, nunca a identidade de quem
     /// falhou, porque quem relata é quem estava esperando a imagem — a
     /// vítima, não quem investiga.
     #[must_use]
     pub fn quem_foi_apontado(&self, screen: ScreenId) -> Option<PersonId> {
-        self.nomeacoes.get(&screen).copied()
+        self.nomeacoes
+            .get(&screen)
+            .map(|nomeacao| nomeacao.empresta)
     }
 }
 
 /// Dado o motivo de um `ClientMessage::ParFalhou` e quem o servidor tinha
 /// apontado para a transmissão, quem (se alguém) desacreditar.
 ///
 /// **Função pura, extraída no fix round 3.** É onde o defeito do round 2
 /// morava — `session.rs` chamava `saiu(session.person)`, que é sempre quem
 /// **relata**, nunca quem falhou — e o round 2 corrigiu isso em `session.rs`
 /// sem deixar um teste que exercitasse a decisão em si: o revisor reintroduziu
@@ -284,36 +386,45 @@ pub fn quem_desacreditar(
     clippy::expect_used,
     reason = "um teste que trata o caso impossível deixa de ser uma afirmação sobre o código"
 )]
 mod testes {
     use super::*;
 
     fn endereco(n: u8) -> SocketAddr {
         SocketAddr::from(([192, 168, 1, n], 8383))
     }
 
+    /// A sala de voz em que todo mundo deste módulo de teste está sentado.
+    ///
+    /// Os testes que não falam de sala nenhuma passam esta: eles afirmam
+    /// outras regras de `escolher`, e uma sala vazia as tornaria vácuas —
+    /// passariam por não haver ninguém na sala, não pela regra sob teste.
+    fn toda_a_sala() -> HashSet<PersonId> {
+        (1..=9).map(PersonId).collect()
+    }
+
     #[test]
     fn quem_compartilha_nunca_e_escolhido_para_servir_a_si_mesmo() {
         // O espelho infinito, na versão da malha: quem compartilha servindo a
         // própria tela a si mesmo. `crate::voice_room` já prende isto para o
         // caminho do servidor; aqui é a mesma regra no caminho novo.
         let mut pares = Pares::nova();
         pares.declarou(
             PersonId(1),
             1,
             true,
             "a".repeat(64),
             vec![endereco(1)],
             endereco(1),
         );
         assert!(pares
-            .escolher(PersonId(1), PersonId(2), &HashSet::new())
+            .escolher(PersonId(1), PersonId(2), &HashSet::new(), &toda_a_sala())
             .is_none());
     }
 
     #[test]
     fn quem_nao_empresta_nunca_e_escolhido_mesmo_com_impressao_guardada() {
         // **A regra nova do fix round 1.** Antes, `impressao` vazia era o
         // sinal de "não empresto" e `declarou` apagava a pessoa inteira. Agora
         // quem só assiste também declara identidade (para poder apresentar
         // certificado quando `SirvaTelaPara` mandar alguém discar para ela) —
         // e a impressão continua guardada mesmo com `emprestando: false`.
@@ -331,59 +442,126 @@ mod testes {
         );
         pares.declarou(
             PersonId(3),
             1,
             false,
             "c".repeat(64),
             vec![endereco(3)],
             endereco(3),
         );
         assert!(pares
-            .escolher(PersonId(1), PersonId(2), &HashSet::new())
+            .escolher(PersonId(1), PersonId(2), &HashSet::new(), &toda_a_sala())
             .is_none());
     }
 
+    #[test]
+    fn quem_empresta_de_outra_sala_de_voz_nunca_e_escolhido() {
+        // **Vazamento de conteúdo entre salas.** `Pares` é global ao daemon:
+        // a pessoa 4 declarou que empresta enquanto estava numa sala, e a
+        // transmissão sob escolha está em outra. Se ela for apontada, o
+        // cliente dela repassa o que **ela** recebe — a tela da sala dela — e
+        // quem assiste na sala da transmissão vê conteúdo de uma sala em que
+        // nunca entrou. A declaração é global; a escolha não pode ser.
+        let mut pares = Pares::nova();
+        pares.declarou(
+            PersonId(4),
+            1,
+            true,
+            "d".repeat(64),
+            vec![endereco(4)],
+            endereco(4),
+        );
+        // Quem compartilha, quem quer assistir — e mais ninguém. A pessoa 4
+        // está noutro lugar.
+        let na_sala = HashSet::from([PersonId(1), PersonId(2)]);
+        assert!(
+            pares
+                .escolher(PersonId(1), PersonId(2), &HashSet::new(), &na_sala)
+                .is_none(),
+            "alguém de outra sala de voz foi apontado para servir esta tela — \
+             o repasse dele carrega a tela da sala dele"
+        );
+    }
+
+    #[test]
+    fn entre_dois_que_emprestam_so_o_da_sala_da_tela_e_escolhido() {
+        // A outra metade do guarda: com um candidato de cada lado, a escolha
+        // não pode ser «o primeiro que o `HashMap` devolver». Sem o filtro,
+        // este teste passaria metade das vezes — e um teste que passa metade
+        // das vezes é o pior guarda que existe.
+        let na_sala = HashSet::from([PersonId(1), PersonId(2), PersonId(4)]);
+        // **Um `Pares` novo a cada rodada, e não um reusado.** A ordem de
+        // iteração de um `HashMap` é fixa enquanto o mapa vive; ela só muda
+        // com a semente, que é sorteada uma vez por mapa. Um laço sobre o
+        // mesmo mapa repetiria cinquenta vezes a mesma ordem — e um guarda
+        // que depende de a ordem ter caído do lado errado é um guarda que
+        // passa metade das vezes.
+        for _ in 0..50 {
+            let mut pares = Pares::nova();
+            for pessoa in [3_u8, 4] {
+                pares.declarou(
+                    PersonId(u64::from(pessoa)),
+                    1,
+                    true,
+                    "c".repeat(64),
+                    vec![endereco(pessoa)],
+                    endereco(pessoa),
+                );
+            }
+            let escolhido = pares
+                .escolher(PersonId(1), PersonId(2), &HashSet::new(), &na_sala)
+                .expect("havia um par elegível na sala da transmissão");
+            assert_eq!(
+                escolhido.pessoa,
+                PersonId(4),
+                "a escolha saiu da sala da transmissão"
+            );
+        }
+    }
+
     #[test]
     fn quem_ja_esta_servindo_nao_e_escolhido_de_novo() {
         // **Um par por vez, no A1.** Quantos um cliente aguenta é a conta do
         // subprojeto B, e supor «dois» aqui seria inventar um número que
         // ninguém mediu.
         let mut pares = Pares::nova();
         pares.declarou(
             PersonId(3),
             1,
             true,
             "c".repeat(64),
             vec![endereco(3)],
             endereco(3),
         );
         let ja = HashSet::from([PersonId(3)]);
-        assert!(pares.escolher(PersonId(1), PersonId(2), &ja).is_none());
+        assert!(pares
+            .escolher(PersonId(1), PersonId(2), &ja, &toda_a_sala())
+            .is_none());
     }
 
     #[test]
     fn o_endereco_publico_vem_do_servidor_e_nao_do_cliente() {
         // Um endereço público que o cliente afirma é um endereço que ele pode
         // mentir — e mentir aqui manda outra pessoa discar para onde o mentiroso
         // quiser. O servidor vê a origem da conexão; é ela que vale.
         let mut pares = Pares::nova();
         let publico = SocketAddr::from(([203, 0, 113, 9], 8383));
         pares.declarou(
             PersonId(3),
             1,
             true,
             "c".repeat(64),
             vec![endereco(3)],
             publico,
         );
         let escolhido = pares
-            .escolher(PersonId(1), PersonId(2), &HashSet::new())
+            .escolher(PersonId(1), PersonId(2), &HashSet::new(), &toda_a_sala())
             .unwrap();
         assert!(escolhido.enderecos.contains(&publico));
         assert!(escolhido.enderecos.contains(&endereco(3)));
     }
 
     #[test]
     fn a_impressao_sobrevive_a_emprestando_false() {
         // **O guarda que faltava no round 1.** O teste anterior
         // (`quem_nao_empresta_nunca_e_escolhido_mesmo_com_impressao_guardada`)
         // só afirma a metade `escolher`; esta prova a outra metade — que
@@ -463,45 +641,113 @@ mod testes {
     #[test]
     fn um_parfalhou_resolve_contra_a_propria_nomeacao_e_nao_contra_quem_relata() {
         // **Achado do fix round 2.** `ParFalhou { screen }` não carrega quem
         // falhou — só quem relata sabe que a imagem parou, e relatar não é o
         // mesmo que saber a identidade do impostor. O servidor tem de
         // resolver `screen` contra a própria nomeação (`apontou`), não contra
         // `session.person` do despacho — esse é sempre quem relatou, a
         // vítima, nunca o par apontado.
         let mut pares = Pares::nova();
         let tela = ScreenId(9);
-        pares.apontou(tela, PersonId(5));
+        pares.apontou(tela, PersonId(5), PersonId(2));
         assert_eq!(pares.quem_foi_apontado(tela), Some(PersonId(5)));
     }
 
     #[test]
     fn quem_sai_deixa_de_ser_a_resposta_de_uma_nomeacao_velha() {
         let mut pares = Pares::nova();
         pares.declarou(
             PersonId(5),
             1,
             true,
             "e".repeat(64),
             vec![endereco(5)],
             endereco(5),
         );
         let tela = ScreenId(9);
-        pares.apontou(tela, PersonId(5));
+        pares.apontou(tela, PersonId(5), PersonId(2));
         pares.saiu(PersonId(5), 1);
         assert_eq!(
             pares.quem_foi_apontado(tela),
             None,
             "quem já foi embora continuou sendo a resposta de uma nomeação"
         );
     }
 
+    #[test]
+    fn quem_assiste_indo_embora_devolve_o_par_a_quem_pode_escolher() {
+        // **Cada repasse encerrado queimava um par para sempre.** A nomeação
+        // só era apagada por `ParFalhou`; quem assiste saindo da sala — ou a
+        // sessão dela acabando — deixava a nomeação de pé, e `ja_servindo`
+        // contava aquele par como ocupado pelo resto da sessão do daemon,
+        // enquanto o cliente dele já tinha devolvido a vaga.
+        let mut pares = Pares::nova();
+        pares.declarou(
+            PersonId(4),
+            1,
+            true,
+            "d".repeat(64),
+            vec![endereco(4)],
+            endereco(4),
+        );
+        pares.apontou(ScreenId(9), PersonId(4), PersonId(2));
+        assert!(
+            pares.ja_servindo().contains(&PersonId(4)),
+            "a nomeação não pôs o par em ja_servindo"
+        );
+
+        pares.quem_assiste_saiu(PersonId(2));
+        assert!(
+            pares.ja_servindo().is_empty(),
+            "quem assiste foi embora e o par apontado continuou contado como ocupado"
+        );
+        assert!(
+            pares
+                .escolher(
+                    PersonId(1),
+                    PersonId(3),
+                    &pares.ja_servindo(),
+                    &toda_a_sala()
+                )
+                .is_some(),
+            "o par não voltou a ser escolhível depois de o repasse acabar"
+        );
+    }
+
+    #[test]
+    fn a_saida_de_quem_assiste_nao_derruba_a_nomeacao_de_outra_pessoa() {
+        // A outra metade: um `retain` escrito ao contrário passaria no teste
+        // acima e desligaria toda nomeação viva a cada saída de sala.
+        let mut pares = Pares::nova();
+        pares.apontou(ScreenId(9), PersonId(4), PersonId(2));
+        pares.quem_assiste_saiu(PersonId(7));
+        assert_eq!(
+            pares.quem_foi_apontado(ScreenId(9)),
+            Some(PersonId(4)),
+            "a saída de quem não assistia esta tela derrubou a nomeação dela"
+        );
+    }
+
+    #[test]
+    fn a_sessao_que_acaba_devolve_o_par_que_servia_esta_pessoa() {
+        // `saiu` confere a conexão para não apagar a **declaração** de uma
+        // conexão que venceu a corrida de candidatos. A nomeação feita para
+        // esta pessoa não é declaração de ninguém: ela cai de qualquer jeito.
+        let mut pares = Pares::nova();
+        pares.apontou(ScreenId(9), PersonId(4), PersonId(2));
+        pares.saiu(PersonId(2), 77);
+        assert!(
+            pares.ja_servindo().is_empty(),
+            "a sessão de quem assiste acabou e o par apontado continuou ocupado"
+        );
+    }
+
     #[test]
     fn desacreditar_apaga_independente_da_conexao() {
         let mut pares = Pares::nova();
         pares.declarou(
             PersonId(3),
             1,
             true,
             "c".repeat(64),
             vec![endereco(3)],
             endereco(3),
diff --git a/crates/seele-server/src/session.rs b/crates/seele-server/src/session.rs
index ea8423f..a82276e 100644
--- a/crates/seele-server/src/session.rs
+++ b/crates/seele-server/src/session.rs
@@ -356,21 +356,21 @@ pub async fn serve(
         &server,
         &voice_rooms,
         &registry,
     )
     .await;
 
     // Out of every room, not out of the one this connection remembers. The loop
     // above can end at any `?`, and a path that returns early does not know
     // where the person was sitting.
     voice_rooms.leave_everywhere(session.person).await;
-    encerrar_telas_de(&server, session.person).await;
+    soltar_telas_e_pares_de(&server, session.person).await;
 
     // And **announced**, which it was not. `Event::PersonLeft` was sent only
     // from the `LeaveVoiceRoom` branch, so a person who closed their client, lost
     // their network or hit any `?` in the loop stayed in everybody else's
     // roster until they reconnected. Nobody saw it while a client only drew the
     // voice room it was sitting in and only learned of that voice room's arrivals; now that
     // every voice room is drawn with the people in it, a ghost is a ghost on screen.
     //
     // Here rather than at the end of `run_session` for the same reason as the
     // channel above: this is the one place every exit path passes through.
@@ -1553,21 +1553,21 @@ async fn run_session(
                                 operator_text: None,
                             }).await?;
                             continue;
                         }
                         assentar(server, voice_rooms, session, &outbound_tx, &tela_tx, id).await?;
                         current_voice_room = Some(id);
                         midia.entrou(current_voice_room);
                     }
                     ClientMessage::LeaveVoiceRoom => {
                         voice_rooms.leave_everywhere(session.person).await;
-                        encerrar_telas_de(server, session.person).await;
+                        soltar_telas_e_pares_de(server, session.person).await;
                         if let Some(id) = current_voice_room.take() {
                             midia.entrou(current_voice_room);
                             server.occupancy.lock().await.vacate(id, session.person);
                             let _ = server.events.send(Event::PersonLeft {
                                 voice_room: id,
                                 person: session.person,
                             });
                         }
                     }
                     ClientMessage::JoinChannel { channel } => {
@@ -2032,20 +2032,21 @@ async fn run_session(
                                 tracing::info!(by = %session.person, voice_room = %id, "voice room destroyed");
                                 // A transmissão morre com a sala, e antes do
                                 // aviso de que a sala morreu: quem está
                                 // desenhando a tela para de desenhá-la porque
                                 // ela acabou, e não porque o cômodo sumiu de
                                 // baixo dela.
                                 // Uma por transmissão: a sala pode ter mais de
                                 // uma em curso, e um aviso só deixaria as outras
                                 // desenhadas para sempre na tela de quem assiste.
                                 for screen in server.telas.lock().await.encerrar_voice_room(id) {
+                                    server.pares.lock().await.desapontou(screen);
                                     let _ = server.events.send(Event::ScreenShareStopped {
                                         voice_room: id,
                                         screen,
                                     });
                                 }
                                 let _ = server.events.send(Event::VoiceRoomDeleted { voice_room: id });
                             }
                             // The only refusal here with a sentence of its own.
                             // Everything else a write can fail with is the
                             // database's business and goes to the operator's log.
@@ -2243,21 +2244,28 @@ async fn run_session(
                                 matches!(message, ClientMessage::WatchScreen { .. });
                             // **O caminho entre pares é perguntado antes do
                             // cano do servidor**, e é o ponto inteiro da malha:
                             // quando um par assume, esta cópia não sai desta
                             // máquina. Quando não há par — ninguém emprestando,
                             // ninguém com identidade declarada, todo mundo já
                             // servindo —, nada muda em relação a antes desta
                             // onda existir, que é o que «a malha é alívio,
                             // nunca dependência» quer dizer no código.
                             let pelo_par = assistir
-                                && apontar_um_par(server, screen, dono, session.person).await;
+                                && apontar_um_par(
+                                    server,
+                                    screen,
+                                    voice_room,
+                                    dono,
+                                    session.person,
+                                )
+                                .await;
                             let comando = if assistir && !pelo_par {
                                 VoiceRoomCommand::TelaAssistir {
                                     person: session.person,
                                     screen,
                                 }
                             } else {
                                 // **Desligado, e não «não ligado».** Quem entra
                                 // numa sala com uma transmissão só já entra
                                 // ligado nela (`VoiceRoom::tela_abriu`), então
                                 // um par que assume encontra o cano do servidor
@@ -2270,28 +2278,41 @@ async fn run_session(
                                 VoiceRoomCommand::TelaParouDeAssistir {
                                     person: session.person,
                                     screen,
                                 }
                             };
                             // O resultado é descartado como nos outros
                             // comandos de sala: a sala que sumiu entre a
                             // conferência e o envio já não tem transmissão para
                             // assistir, e não há o que dizer a quem pediu.
                             let _ = voice_rooms.of(voice_room).await.send(comando).await;
+                            if !assistir {
+                                // **O fim bom do repasse, e ele também solta o
+                                // par.** Enquanto só `ParFalhou` chamava
+                                // `desapontou`, um repasse que terminasse bem
+                                // deixava a nomeação de pé e `ja_servindo`
+                                // contava aquele par como ocupado pelo resto
+                                // da sessão do daemon — com o cliente dele já
+                                // tendo devolvido a vaga.
+                                server.pares.lock().await.desapontou(screen);
+                            }
                         }
                     }
                     ClientMessage::StopScreenShare => {
                         let parada = match current_voice_room {
                             Some(voice_room) => server.telas.lock().await.parar(voice_room, session.person),
                             None => None,
                         };
                         if let (Some(voice_room), Some(screen)) = (current_voice_room, parada) {
+                            // A transmissão acabou: não há repasse dela para
+                            // ninguém, e o par que a servia volta à fila.
+                            server.pares.lock().await.desapontou(screen);
                             let _ = server.events.send(Event::ScreenShareStopped { voice_room, screen });
                         }
                     }
                     ClientMessage::RequestKeyFrame { screen } => {
                         // Conferido contra o registro, e não repassado ao
                         // acaso: um `ScreenId` inventado seria uma maneira de
                         // pedir quadro-chave a quem transmite em outra sala, e
                         // §3.3 conta o que um quadro-chave custa — 65 KiB em
                         // 1080p, 446 ms do orçamento inteiro. Um pedido que
                         // atravessasse salas seria amplificação de graça.
@@ -2606,21 +2627,21 @@ async fn run_session(
                     // bookkeeping `LeaveVoiceRoom` does, because it is the same
                     // thing happening without being asked for — and only then
                     // does this client hear that the room is gone.
                     //
                     // Order matters in the other direction too: `PersonLeft`
                     // goes out before `VoiceRoomDeleted` reaches anybody, so no
                     // client is ever holding a roster for a room it has already
                     // been told to forget.
                     Event::VoiceRoomDeleted { voice_room: id } if current_voice_room == Some(*id) => {
                         voice_rooms.leave_everywhere(session.person).await;
-                        encerrar_telas_de(server, session.person).await;
+                        soltar_telas_e_pares_de(server, session.person).await;
                         current_voice_room = None;
                         midia.entrou(current_voice_room);
                         server.occupancy.lock().await.vacate(*id, session.person);
                         let _ = server.events.send(Event::PersonLeft {
                             voice_room: *id,
                             person: session.person,
                         });
                         frame::write(&mut send, &ServerMessage::VoiceRoomDeleted {
                             voice_room: *id,
                         }).await?;
@@ -2650,32 +2671,21 @@ async fn run_session(
                             severity: AlertSeverity::Warning,
                             reason: AlertReason::ChannelDeleted,
                             operator_text: None,
                         }).await?;
                         continue;
                     }
                     _ => {}
                 }
 
                 if let Some(message) = translate(&event, &channels, session.person) {
-                    // Um cliente v1 não conhece as variantes que a v2
-                    // acrescentou, e o postcard não é autodescritivo: mandá-la
-                    // não seria ignorada do outro lado — deslocaria o fluxo de
-                    // controle dele para sempre, e a partir dali ele segue
-                    // conectado sem entender mais nenhum quadro. A janela de
-                    // compatibilidade do ADR 0036 promete que ele continua
-                    // funcionando, e esta é a linha que cumpre a promessa.
-                    let entende = match message {
-                        ServerMessage::UplinkLoss { .. } => session.protocol_version >= 2,
-                        _ => true,
-                    };
-                    if entende {
+                    if entende_a_mensagem(&message, session.protocol_version) {
                         frame::write(&mut send, &message).await?;
                     }
                 }
             }
 
             _ = telemetry.tick() => {
                 // The server measures RTT and loss from QUIC itself, which is
                 // the only vantage point that sees both directions. Jitter is
                 // measured at the receiver, so the server reports zero rather
                 // than a number it cannot know.
@@ -2833,21 +2843,21 @@ async fn assentar(
 ) -> Result<()> {
     // Out of the old room before into the new one. Without this a person who
     // walks from one voice room to another is still a member of the first, and goes
     // on hearing it.
     voice_rooms.leave_everywhere(session.person).await;
     // E a tela vai junto. Uma transmissão não anda de sala com quem a manda:
     // quem ficou na sala anterior continuaria vendo o cabeçalho de um fluxo que
     // agora aponta para outro lugar, e o §6 item 3 só permite uma por sala —
     // levar a transmissão pela mão faria a pessoa tomar a vaga da sala nova sem
     // ter pedido.
-    encerrar_telas_de(server, session.person).await;
+    soltar_telas_e_pares_de(server, session.person).await;
     voice_rooms
         .of(destino)
         .await
         .send(VoiceRoomCommand::Join {
             person: session.person,
             ssrc: session.ssrc,
             may_speak: session.may_speak,
             outbound: outbound.clone(),
             tela: tela.clone(),
         })
@@ -3005,26 +3015,68 @@ async fn moderavel(
     // failing must not answer "nobody here is an administrator".
     let alvo_administra = permissions
         .may(alvo, Permission::AdministerServer)
         .unwrap_or(true);
     let quem_administra = permissions
         .may(quem, Permission::AdministerServer)
         .unwrap_or(false);
     !alvo_administra || quem_administra
 }
 
+/// Se um cliente nesta versão de protocolo entende esta mensagem.
+///
+/// **Uma mensagem que o outro lado não conhece não é ignorada por ele.** O
+/// postcard não é autodescritivo: um cliente v3 que receba uma variante da v4
+/// lê os bytes dela como se fossem de outra coisa, e o fluxo de controle dele
+/// fica deslocado **para sempre** — a partir dali ele segue conectado sem
+/// entender mais nenhum quadro. A janela de compatibilidade do ADR 0036
+/// promete que um cliente dentro dela continua funcionando, e é esta função
+/// que cumpre a promessa.
+///
+/// # Por que função pura, e não um `match` dentro do laço
+///
+/// Porque a promessa é sobre um cliente que este servidor não tem em teste
+/// nenhum. Dentro do laço, a única forma de exercitá-la seria montar uma
+/// sessão v3 inteira; extraída, a regra é afirmável mensagem a mensagem — que
+/// é a mesma razão de `crate::pares::quem_desacreditar` ter saído do braço de
+/// `ParFalhou`.
+///
+/// # A linha que faltava
+///
+/// `SirvaTelaPara` e `AssistaTelaPor` são da v4 e caíam no `_ => true`. Hoje
+/// um cliente v3 não as recebe — `Pares::escolher` exige `emprestando: true`,
+/// que só chega por `EmprestarSubida`, que é v4 —, mas isso é um invariante de
+/// outra tarefa, e não uma linha que afirme a promessa. Estas duas afirmam.
+#[must_use]
+fn entende_a_mensagem(message: &ServerMessage, versao: u8) -> bool {
+    match message {
+        // A v2 acrescentou esta. O `>= 2` de antes já era vácuo — a janela de
+        // compatibilidade do ADR 0036 é de uma versão, e
+        // `oldest_supported_version()` já é 3, então nenhuma sessão viva chega
+        // aqui com menos que isso. Fica escrito com a versão em que a variante
+        // nasceu, e não apagado: no dia em que a janela alargar, é este número
+        // que volta a morder, e reconstruí-lo por arqueologia custaria mais do
+        // que a linha custa.
+        ServerMessage::UplinkLoss { .. } => versao >= 2,
+        // As duas da v4, o caminho entre pares.
+        ServerMessage::SirvaTelaPara { .. } | ServerMessage::AssistaTelaPor { .. } => versao >= 4,
+        _ => true,
+    }
+}
+
 /// Tenta pôr um par a servir esta transmissão a quem acabou de pedir para vê-la.
 ///
 /// `true` quando um par foi apontado e as duas mensagens saíram — e é o `true`
 /// que faz o braço de `WatchScreen` **não** abrir o cano do servidor para esta
 /// pessoa. `false` é o caminho de sempre, e ele é a maioria dos casos: ninguém
-/// emprestando na sala, ninguém livre, ou quem pediu sem identidade declarada.
+/// emprestando **nesta sala**, ninguém livre, ou quem pediu sem identidade
+/// declarada.
 ///
 /// # As duas declarações, e por que as duas
 ///
 /// Uma ligação entre pares tem parede simétrica (Task 5): quem atende confere
 /// quem chega contra a impressão que o servidor apresentou, e quem disca
 /// confere quem atendeu contra a impressão que o servidor apresentou. Então o
 /// servidor precisa das **duas** declarações — a de quem empresta, que
 /// [`crate::pares::Pares::escolher`] devolve, e a de quem pediu, que só
 /// [`crate::pares::Pares::declaracao_de`] tem. Sem a segunda, `SirvaTelaPara`
 /// sairia sem impressão a conferir e quem empresta recusaria a ligação de quem
@@ -3034,41 +3086,61 @@ async fn moderavel(
 ///
 /// Quem empresta precisa **passar a atender** antes de quem assiste discar:
 /// `crate::pares` não guarda ninguém escutando, e a porta de quem empresta só
 /// é armada quando a mensagem chega lá. Os dois avisos saem no mesmo instante
 /// pelo mesmo barramento, então esta ordem é só o que dá a quem empresta a
 /// dianteira que ele precisa — quem assiste tenta de novo dentro do prazo se
 /// chegar cedo demais.
 async fn apontar_um_par(
     server: &Server,
     screen: ScreenId,
+    voice_room: VoiceRoomId,
     dono: PersonId,
     quem_quer: PersonId,
 ) -> bool {
+    // **Quem está na sala agora, perguntado agora.** `crate::pares::Pares` é
+    // global ao daemon — uma pessoa declara identidade uma vez por sessão, não
+    // uma vez por sala —, então sem esta pergunta quem empresta na sala B seria
+    // apontado para servir a tela da sala A, e repassaria a tela da sala B a
+    // quem nunca entrou nela. A fonte é a `Occupancy`, que é reescrita a cada
+    // entrada e saída; guardar a sala dentro da declaração daria um valor
+    // escrito uma vez e nunca mais.
+    //
+    // Lido **antes** de tomar `pares`, e não dentro: dois mutexes tomados na
+    // mesma ordem em todo lugar são uma ordem; tomados em ordens diferentes
+    // são um travamento esperando o primeiro dia ruim.
+    let na_sala: std::collections::HashSet<PersonId> = server
+        .occupancy
+        .lock()
+        .await
+        .in_voice_room(voice_room)
+        .into_iter()
+        .map(|ocupante| ocupante.person)
+        .collect();
     let (empresta, quem) = {
         let mut pares = server.pares.lock().await;
         let ja_servindo = pares.ja_servindo();
-        let Some(empresta) = pares.escolher(dono, quem_quer, &ja_servindo) else {
+        let Some(empresta) = pares.escolher(dono, quem_quer, &ja_servindo, &na_sala) else {
             return false;
         };
         let Some(quem) = pares.declaracao_de(quem_quer).cloned() else {
             // Quem nunca declarou identidade não tem impressão para
             // apresentar, e a parede simétrica recusaria a ligação. Cair para o
             // servidor é o certo, e o rastro diz de quem se fala.
             tracing::debug!(
                 person = %quem_quer,
                 %screen,
                 "quem pediu para assistir não declarou identidade de par; a tela vem do servidor"
             );
             return false;
         };
-        pares.apontou(screen, empresta.pessoa);
+        pares.apontou(screen, empresta.pessoa, quem_quer);
         (empresta, quem)
     };
     tracing::info!(
         %screen,
         empresta = %empresta.pessoa,
         assiste = %quem_quer,
         enderecos = ?empresta.enderecos,
         "o servidor apontou um par para servir esta transmissão"
     );
     let _ = server.events.send(Event::SirvaTelaPara {
@@ -3147,21 +3219,21 @@ async fn receber_tela(
         // Consultado entre duas leituras e não dentro de um `select!`: a
         // leitura do `quinn` não é cancel-safe, e um `select!` que a cancelasse
         // no meio perderia os bytes já retirados do fluxo — o mesmo defeito que
         // a tarefa leitora do controle existe para não ter.
         if let Ok(motivo) = fim_rx.try_recv() {
             tracing::info!(%person, %voice_room, %screen, ?motivo, "o servidor encerrou uma transmissão");
             let _ = fluxo.stop(quinn::VarInt::from_u32(crate::tela::CODIGO_DE_CORTE));
             // Anunciado, porque o plano de controle é o único lugar de onde a
             // sala aprende que a tela parou. Sem isto ficaria desenhada uma
             // transmissão que já não tem quem a bombeie.
-            encerrar_telas_de(server, person).await;
+            soltar_telas_e_pares_de(server, person).await;
             // E com nome, para quem a mandava. `ScreenShareStopped` vai para a
             // sala inteira e não carrega razão de propósito — as duas maneiras
             // comuns de acabar já se distinguem sozinhas —, mas esta terceira
             // não: quem apertou parar sabe que apertou, e quem foi parado pelo
             // servidor não descobriria nada. A frase que falta é «a sala cresceu
             // além do que esta subida carrega», e ela é do §5.1.
             //
             // Pela sessão de quem compartilha, que é a única a quem ela
             // interessa, e pelo barramento de avisos: escrever no fluxo de
             // controle daqui exigiria a caneta que o laço da sessão segura.
@@ -3198,30 +3270,49 @@ async fn receber_tela(
     }
     // O fim limpo. Quem parou de propósito também manda `StopScreenShare` pelo
     // controle, e é ele que anuncia; quem sumiu é recolhido pelo fim da sessão.
     // Aqui só o encaminhamento morre, que é o que o §5.1 pôs sob esta função.
     let _ = sala
         .send(VoiceRoomCommand::TelaFechou { from: person })
         .await;
     Ok(())
 }
 
-/// Encerra e anuncia o que esta pessoa estivesse transmitindo, onde estivesse.
+/// Encerra e anuncia o que esta pessoa estivesse transmitindo, e solta os pares
+/// que ela sustentava.
 ///
 /// Chamado em todo lugar onde alguém sai de uma sala de voz — sair, ser movido, ser
 /// expulso, ou a conexão acabar em qualquer `?` do meio do laço. Uma
 /// transmissão que sobrevivesse à saída de quem a manda ficaria desenhada para
 /// sempre na sala, prometendo um fluxo que não tem mais de onde vir: é o mesmo
 /// defeito da pessoa fantasma que `serve` conserta logo acima, com a diferença
 /// de que aqui a promessa é de imagem em movimento.
-async fn encerrar_telas_de(server: &Server, person: PersonId) {
-    for (voice_room, screen) in server.telas.lock().await.encerrar_de(person) {
+///
+/// # E as nomeações de par, nas duas direções
+///
+/// Sair da sala encerra um repasse tanto quanto um `UnwatchScreen`, e por dois
+/// caminhos: as transmissões **desta** pessoa acabaram (então o par que as
+/// servia está livre), e o que **ela** assistia acabou para ela (então o par
+/// apontado para lhe servir está livre). Sem as duas linhas, cada saída
+/// deixaria uma nomeação de pé, `crate::pares::Pares::ja_servindo` contaria
+/// aquele par como ocupado pelo resto da sessão do daemon, e a malha
+/// degradaria para a estrela sem um rastro.
+async fn soltar_telas_e_pares_de(server: &Server, person: PersonId) {
+    let encerradas = server.telas.lock().await.encerrar_de(person);
+    {
+        let mut pares = server.pares.lock().await;
+        for (_, screen) in &encerradas {
+            pares.desapontou(*screen);
+        }
+        pares.quem_assiste_saiu(person);
+    }
+    for (voice_room, screen) in encerradas {
         let _ = server
             .events
             .send(Event::ScreenShareStopped { voice_room, screen });
     }
 }
 
 /// Tells a client the server said no, and why.
 async fn recusar(send: &mut quinn::SendStream, person: PersonId, verbo: &str) -> Result<()> {
     tracing::warn!(%person, verbo, "refused: the server said no");
     frame::write(
@@ -3632,10 +3723,85 @@ mod plano_de_midia {
              mostra a mudança, então nada na tela contradiz o defeito."
         );
         // Se as trocas sumirem, este guarda passa a não guardar nada e ninguém
         // percebe. O número não precisa estar certo; precisa não ser zero.
         assert!(
             atribuicoes >= 6,
             "só {atribuicoes} trocas de sala encontradas — o guarda perdeu o alvo"
         );
     }
 }
+
+#[cfg(test)]
+#[allow(
+    clippy::unwrap_used,
+    clippy::expect_used,
+    reason = "num teste, o pânico é o relatório"
+)]
+mod versao_no_fio {
+    use super::*;
+
+    fn sirva() -> ServerMessage {
+        ServerMessage::SirvaTelaPara {
+            screen: ScreenId(7),
+            enderecos: vec![std::net::SocketAddr::from(([203, 0, 113, 9], 8383))],
+            impressao: "a".repeat(64),
+        }
+    }
+
+    fn assista() -> ServerMessage {
+        ServerMessage::AssistaTelaPor {
+            screen: ScreenId(7),
+            enderecos: vec![std::net::SocketAddr::from(([203, 0, 113, 9], 8383))],
+            impressao: "a".repeat(64),
+        }
+    }
+
+    #[test]
+    fn as_duas_mensagens_da_v4_nao_saem_para_um_cliente_v3() {
+        // **A promessa do ADR 0036, escrita numa linha em vez de suposta.** Um
+        // cliente v3 que recebesse uma destas leria os bytes dela como se
+        // fossem de outra coisa — o postcard não é autodescritivo — e o fluxo
+        // de controle dele ficaria deslocado para sempre.
+        //
+        // Que ele não as receba hoje é verdade **por acidente**: `escolher`
+        // exige `emprestando: true`, e isso só chega por `EmprestarSubida`,
+        // que é v4. Um invariante de outra tarefa não é a promessa; estas
+        // asserções são.
+        for versao in [seele_proto::version::oldest_supported_version(), 3] {
+            assert!(
+                !entende_a_mensagem(&sirva(), versao),
+                "SirvaTelaPara saiu para um cliente v{versao}"
+            );
+            assert!(
+                !entende_a_mensagem(&assista(), versao),
+                "AssistaTelaPor saiu para um cliente v{versao}"
+            );
+        }
+    }
+
+    #[test]
+    fn as_duas_mensagens_da_v4_saem_para_um_cliente_v4() {
+        // A outra metade: um guarda que barrasse todo mundo passaria no teste
+        // acima e desligaria a malha inteira sem um rastro.
+        assert!(entende_a_mensagem(
+            &sirva(),
+            seele_proto::version::PROTOCOL_VERSION
+        ));
+        assert!(entende_a_mensagem(
+            &assista(),
+            seele_proto::version::PROTOCOL_VERSION
+        ));
+    }
+
+    #[test]
+    fn a_versao_mais_velha_ainda_aceita_recebe_tudo_o_que_nao_e_da_v4() {
+        // O `>= 2` de `UplinkLoss` é vácuo desde que a janela do ADR 0036
+        // subiu o piso para 3 — este teste é o que o diz em voz alta, e é o
+        // que vai falhar no dia em que a janela alargar e ele deixar de ser.
+        assert_eq!(seele_proto::version::oldest_supported_version(), 3);
+        assert!(entende_a_mensagem(
+            &ServerMessage::UplinkLoss { fraction: 0.0 },
+            seele_proto::version::oldest_supported_version()
+        ));
+    }
+}
diff --git a/docs/superpowers/specs/2026-09-05-caminho-entre-pares-design.md b/docs/superpowers/specs/2026-09-05-caminho-entre-pares-design.md
index d5b5215..4351333 100644
--- a/docs/superpowers/specs/2026-09-05-caminho-entre-pares-design.md
+++ b/docs/superpowers/specs/2026-09-05-caminho-entre-pares-design.md
@@ -168,20 +168,50 @@ Três coisas boas caem daqui, e nenhuma custa código:
 - **só a API pública do `quinn`** — sem socket cru, sem pacote inventado.
 
 **O ponto de encontro não participa, e isto é uma revisão do que se pensou
 primeiro.** A ideia inicial era reusar o `LEVE` do degrau 4. Ele é o
 intermediário errado aqui: existe para apresentar duas máquinas que não têm nada
 em comum, e estas têm o servidor — conectado às duas neste instante, e vendo o
 endereço público de cada uma como origem da conexão. Pedir a um terceiro que
 descubra o que o segundo já sabe é um salto de rede e um serviço a mais no
 caminho crítico, por nada.
 
+### 3.3.1 · Um disca, o outro atende — revisão de 07/09/2026
+
+**Escrito depois de a revisão final ler o código que o §3.3 descreve.** Os dois
+lados discam, sim — mas **só um dos dois atende**, e o desenho acima implica
+que qualquer um pode aceitar. O código nunca fez isso, e a diferença importa.
+
+Quem **empresta** passa a atender (`par::passar_a_atender`) e depois disca.
+Quem **assiste** apenas disca: `Motor::assistir_por_par` não chama
+`par::atender` em ponto nenhum. Então a ligação que fecha é sempre a de quem
+assiste chegando a quem empresta; a discagem de quem empresta **não pode**
+fechar, porque não há quem a atenda do outro lado.
+
+Ela continua existindo, e continua sendo metade do furo: são as tentativas de
+conexão dela que abrem o mapeamento de NAT do lado de quem empresta, para a
+discagem de quem assiste entrar por ele. Ela existe pelo efeito, não pelo
+resultado.
+
+**O que isto custou.** Enquanto o desenho dizia «os dois discam», o código
+esperava as duas e tomava a primeira que terminasse como resposta — e um erro
+rápido da discagem de quem empresta (família de endereço incompatível,
+`connect_with` recusando na hora, todos os candidatos falhando) cancelava o
+atendimento e fazia quem empresta desistir de servir alguém que estava
+chegando. O conserto é o desenho real escrito no código: o braço da discagem
+não decide nada, e o prazo é o de quem atende.
+
+As três coisas boas do §3.3 continuam valendo, com uma correção na primeira: o
+caso assimétrico se resolve sozinho **enquanto quem assiste conseguir sair** —
+se só quem empresta conseguir, a ligação não acontece e a tela vem do servidor,
+que é o caminho de sempre.
+
 ### 3.4 · O que já é agnóstico, e por isso não entra na conta
 
 O lado que recebe tela **já não sabe de onde o fluxo vem**:
 `tela::TelaRecebida::do_fluxo` aceita qualquer `quinn::RecvStream`, e quem envia
 escreve num `SendStream` qualquer. O enquadramento, o cabeçalho, o quadro-chave
 e o byte de `StreamType::Screen` funcionam entre dois clientes sem uma linha de
 mudança.
 
 O que falta é a **conexão**, e só ela. É por isso que este subprojeto é de
 transporte e não de mídia.
@@ -202,21 +232,21 @@ por posição — acrescentar no fim é ilegível para quem não conhece a varia
 
 A `impressao` das quatro mensagens é uma `String`: o **SHA-256 do certificado
 DER em hexadecimal minúsculo**, exatamente o que
 `seele_proto::transport::certificate_fingerprint` devolve, que é o que
 `tls::Identity::fingerprint` usa e o que o `fp=` do `seele://` carrega. Um
 formato só para a mesma coisa, em vez de um segundo jeito de dizer «este é o
 certificado» — e `String` e não `[u8; 32]` porque é assim que o pino do ADR 0003
 já viaja e é guardado, e dois formatos para o mesmo hash é o começo de os dois
 discordarem.
 
-- `EmprestarSubida { emprestando: bool, impressao: [u8; 32], locais: Vec<SocketAddr> }`
+- `EmprestarSubida { emprestando: bool, impressao: String, locais: Vec<SocketAddr> }`
   — «eu empresto, este é o meu certificado, e estes são os meus endereços de rede
   local». O endereço público **não** vai aqui: ele é a origem da conexão que já
   está aberta, e o servidor o tem sem perguntar. Um endereço público que o
   cliente afirma seria um endereço que ele pode mentir.
 - `ParFalhou { screen: ScreenId, motivo: MotivoDeFalhaDePar }` — **mandada por
   quem recebe**, e nunca por quem empresta: quem sabe que a imagem parou é quem
   estava esperando por ela, e quem empresta pode ter caído sem chegar a saber de
   nada. Enumerado, como todo motivo deste protocolo
   (`specs/02-protocolo.md`), com cada variante carregando o que a casca precisa
   para escrever a própria frase (ADR 0012). Quatro para começar, e cada uma
@@ -228,29 +258,50 @@ discordarem.
   | `ImpressaoNaoBate` | alcançou, e o certificado não era o apresentado | nunca mais aquele par, e é evento de segurança |
   | `CaiuNoMeio` | estava servindo e a conexão morreu | pode voltar a ser pai depois |
   | `ParouDeMandar` | conexão viva, quadro nenhum dentro do prazo | pode voltar a ser pai depois |
 
   **`ImpressaoNaoBate` não é o mesmo que `NaoAlcancou`**, e juntá-las seria o
   defeito que o ADR 0003 nomeia em outro lugar: a diferença entre «não consegui
   falar com ele» e «alguém respondeu no lugar dele» é a informação inteira.
 
 **servidor → cliente**
 
-- `SirvaTelaPara { screen: ScreenId, enderecos: Vec<SocketAddr>, impressao: [u8; 32] }`
+- `SirvaTelaPara { screen: ScreenId, enderecos: Vec<SocketAddr>, impressao: String }`
   — para quem empresta.
-- `AssistaTelaPor { screen: ScreenId, enderecos: Vec<SocketAddr>, impressao: [u8; 32] }`
+- `AssistaTelaPor { screen: ScreenId, enderecos: Vec<SocketAddr>, impressao: String }`
   — para quem recebe.
 
 As duas são simétricas de propósito: os dois lados fazem a mesma coisa com elas
 — discar para os endereços e conferir a impressão digital —, e a assimetria fica
 só em quem já tem os bytes.
 
+### 4.1 · A impressão é `String` nas assinaturas também — emenda de 07/09/2026
+
+**Escrito depois de a revisão final ler as três assinaturas acima.** Elas diziam
+`impressao: [u8; 32]` enquanto o parágrafo logo antes delas — e o código —
+diziam `String`. O parágrafo estava certo e as assinaturas estavam erradas; as
+três foram corrigidas.
+
+Não é cosmética: o texto que as assinaturas contradiziam é justamente o que
+explica **por que** `String` — «é assim que o pino do ADR 0003 já viaja e é
+guardado, e dois formatos para o mesmo hash é o começo de os dois discordarem».
+Uma spec que carregava os dois formatos já era a primeira das duas
+discordâncias.
+
+**E o tamanho é regra, não descrição.** «Exatamente 64 caracteres» virou
+validação de fio: `seele_proto::control::check_impressao` exige 64 dígitos
+hexadecimais, nem mais nem menos. O teto que havia antes (`<= 64`) aceitava
+`""` e aceitava lixo — e um cliente que declarasse lixo era escolhido por
+`Pares::escolher`, ocupava vaga em `ja_servindo` e custava segundos de tela
+parada a cada `WatchScreen`. Maiúsculas passam na entrada, como `crate::uri` já
+faz com o `fp=` do `seele://`; quem **produz** continua escrevendo minúsculo.
+
 ## 5 · O opt-in entra agora, e a razão é de custo
 
 Quem desenha o produto decidiu em 05/09/2026: **emprestar a subida é escolha de
 quem empresta, e quem entra na sala é avisado de que a malha está ligada.**
 
 O opt-in é escolha por duas razões independentes, e vale separá-las porque só a
 primeira é óbvia:
 
 1. **privacidade** — numa malha, espectadores passam a conhecer o endereço IP uns
    dos outros. Hoje não conhecem: nada em `control.rs` expõe endereço de membro,
