# Diretriz para Claude após a sonda E1

Revisão do código em `1900b37` e do relato de medição no WKWebView. Esta diretriz complementa o [contrato principal](../contrato-mods-api-propria-para-claude.md) e distingue orientação de implementação de comportamento já demonstrado. Não é um novo relatório de teste nativo.

## 1. Próximo passo definido

**Prototipar QuickJS nativo no cliente e continuar a infraestrutura de E2 independente do executor.** O Worker de Blob atual não satisfaz o contrato: a marca de IndexedDB encontrada após reiniciar demonstra persistência fora do controle da sessão, conforme o registro de execução. Não publicar a garantia de isolamento com esse executor.

Manter a WebView existente, o renderer confiável compartilhado e a API própria. Não abrir uma WebView por MOD. Não desenvolver uma biblioteca visual extensa antes da validação do executor. Não manter duas opções públicas de execução por precaução.

Origem isolada para Worker permanece uma alternativa caso o protótipo QuickJS falhe nos requisitos medidos. Uma origem simplesmente diferente da janela não resolve tudo: é necessário demonstrar separação entre instâncias/servidores, ausência de IPC privilegiado e política de armazenamento e encerramento. Uma origem compartilhada pelos MODs ainda permite comunicação entre eles. Essa alternativa também exigiria medir o custo do contexto auxiliar que cria os Workers.

**E1 continua com decisão final pendente.** A orientação escolhe o próximo experimento; não transforma QuickJS em executor aprovado antes das provas.

## 2. QuickJS nativo e o código que já existe

O projeto usa `rquickjs` em `crates/seele-server/Cargo.toml`, que documenta a compilação do motor em C. `crates/seele-server/src/mods/mod.rs`, em `carregar`, cria `Runtime::new()`, configura `set_memory_limit` e `set_interrupt_handler` e cria o contexto.

Portanto, o candidato aqui é um motor nativo acessado por Rust, **não um interpretador implementado em JavaScript dentro da WebView**. A lógica deve executar fora da thread de UI, das threads críticas de voz e do executor assíncrono que atende a sessão. O renderer já seria responsável pela apresentação na proposta de Worker com API própria: trocar o executor não transfere automaticamente um novo custo visual para ele. Muda o custo da lógica, da ponte e dos recursos internos do motor, que precisa ser medido.

Reaproveitar a experiência com limites, não os poderes da metade de servidor. Não importar os bindings de disco, rede ou persistência do servidor para o cliente. Não criar uma dependência de conveniência que viole as fronteiras verificadas por `check-deps`.

Requisitos para o protótipo:

- Um runtime com heap e contexto próprios por instância, pertencente à sua geração de sessão. O número e a organização das threads são uma decisão interna a medir; respeitar as regras de afinidade do binding.
- Somente bindings explícitos da API SEELE. Sem loaders de arquivos arbitrários, módulos de sistema, rede geral, IPC geral ou armazenamento ambiental. Bibliotecas puras podem vir no pacote validado.
- Identidade capturada pelo binding/anfitrião, sem aceitar uma identidade de servidor ou MOD escolhida pela mensagem do autor.
- Fila limitada, justiça entre MODs e limite para execução síncrona e jobs de Promises. O mecanismo de interrupção deve observar revogação e prazo; só contar consultas do motor não estabelece prazo real de saída.
- Bindings curtos, com I/O assíncrono cancelável fora do motor. Uma função nativa bloqueante não é interrompida apenas pelo interrupt handler de JavaScript.
- Descarte no proprietário do runtime, liberando jobs, callbacks, timers e handles. Não tentar matar uma thread Rust à força. Revogação impede novos efeitos imediatamente; o encerramento físico precisa ser medido e confirmado.
- Limites distintos para heap do motor, buffers/filas nativos e recursos visuais. O teto de heap do QuickJS não limita a memória total do MOD.

O código de servidor e seus números históricos não provam o custo desse executor no cliente. Também não oferecem isolamento de processo contra bugs do próprio motor: a garantia é sobre a autoridade oferecida ao código do MOD e os recursos supervisionados.

## 3. O que E2 pode entregar agora

Implementar o supervisor de sessão, identidade/geração, registro de recursos, cancelamento, captura da conexão original e rejeição de resultados antigos. Manter a implementação atrás de um contrato interno de executor: iniciar, entregar evento, solicitar encerramento e confirmar encerramento; os nomes concretos ficam a cargo da organização do projeto.

A instância sai de `ativa` para `encerrando` antes de cancelar ou aguardar qualquer operação. Mensagens e efeitos visuais/nativos passam pela conferência de autoridade. A conclusão do executor é diferente da revogação: até confirmar o descarte dos recursos, não anunciar que tudo foi limpo.

Exercitar saída local, expulsão, fim remoto, troca A→B, reentrada no mesmo servidor, carregamento e resposta atrasados. Um executor simulado ajuda a criar corridas; o Worker atual ajuda a exercitar caminhos existentes. Nenhum deles prova o isolamento do futuro executor. **O aceite integrado de E2 exige repetir os testes com o executor escolhido e resolver E1.**

Pedidos já aceitos pelo servidor antes da revogação podem terminar no servidor. Não prometer desfazer gravações válidas nem apagar campanhas, perfis ou temas compartilhados quando um participante sai. A proibição é de novas admissões locais após revogação e de efeitos tardios na sessão seguinte; os dados persistentes do servidor seguem seu próprio contrato.

## 4. Ajustes necessários na prova E1

Conferido em `apps/seele-app/testes/sonda-de-fronteira/cliente/main.js` e no [registro](registro-de-execucao.md). Corrigir a classificação da evidência antes de usá-la como aceite:

| Afirmação | Limite do instrumento atual | Prova necessária |
| --- | --- | --- |
| Rede externa bloqueada por CSP | O destino é `https://example.invalid/sonda`; `TypeError` também ocorre por falha de DNS/rede/CORS | Destino controlado alcançável, controle positivo e evidência que distinga CSP de falha de transporte |
| Dois MODs conversam por BroadcastChannel | A sonda abre, posta e fecha um único endpoint | Duas instâncias distintas, recebimento e confirmação de um nonce; separar disponibilidade de comunicação comprovada |
| Cache persiste após reiniciar | A sonda chama somente `caches.open` | Escrever, ler e reler após reinício com identificação da execução; a prova de persistência relatada hoje é de IndexedDB |
| Filho morreu junto do pai | Os 2,4 s são relativos à desativação do MOD, sem timestamp do `terminate`; a leitura ocorre depois de criar um novo filho que usa a mesma chave | Registrar revogação e término, identificar gerações e ler a execução antiga antes de iniciar a nova; distinguir execução de commit atrasado de transação |
| IPC autorizado | O pedido usa comando inexistente e retorna HTTP 500 | Registrar como rota alcançável, execução não demonstrada; testar comando inofensivo instrumentado, com controle positivo fora do MOD e sem entregar a chave a ele |
| Caminho recusado | Timeout e outros erros viram `alcancou: false`, guardando só `erro.name` | Separar ausência, recusa, falha e inconclusivo/timeout; guardar causa e limpar operações pendentes |

Para IndexedDB, aguardar a conclusão da transação de escrita, além de `onsuccess` do pedido. Guardar logs brutos, identificação de build/motor, origem, comandos, marcas por execução e resultado de cada controle junto ao relato. A marca relida após reiniciar permanece evidência relevante; esses ajustes não tornam o Worker atual aprovado.

O HTTP 500 não demonstra quebra da chave do Tauri. A chave estar na janela que constrói o Worker tampouco demonstra vazamento. A observação é que a rota é alcançável e a autorização depende de outro mecanismo. Da mesma forma, não justificar a insuficiência de `delete` dizendo que código executado depois do prelúdio guardou referências antes dele: o contrato exige uma fronteira verificável e não aceita essa técnica, mas essa explicação temporal não é uma prova.

## 5. Condição para escolher definitivamente

**Continuação após o protótipo de `c81820f`:** medir primeiro o custo básico do executor (zero/uma/três instâncias, ociosas e sob carga, incluindo saída), depois construir a fatia experimental integrada. Não aguardar o aceite definitivo de E1 para construir essa fatia: ela é parte da evidência necessária para o aceite. A restrição continua sendo não avançar para uma biblioteca visual ampla, migração completa ou publicação antes da decisão.

`cfg(test)` serve aos testes do motor, mas não é condição obrigatória para todo experimento. Permitir uma configuração explícita de bancada que execute QuickJS no aplicativo nativo, em ambiente descartável, sem oferecer uma seleção pública de executores ou ativá-lo na distribuição normal. Integrar o mesmo executor testado ao supervisor de E2, à ponte restrita e ao renderer existente; não criar uma implementação separada só para o benchmark. A regra contra manter dois executores públicos não proíbe um caminho experimental realmente usado pela medição.

Antes da integração, limitar as filas: em `c81820f`, `ExecutorQuickJs::novo` usa `std::sync::mpsc::channel` nos dois sentidos; `seele.postar` limita cada mensagem a 12 KiB, mas não o total acumulado. Definir limites de quantidade e bytes, retorno explícito para saturação e política de encerramento. Não substituir por envio bloqueante que possa prender a thread dentro de um binding e impedir a interrupção. Testar inundação com consumidor parado/lento e confirmar descarte. Mensagens de controle e confirmação de parada precisam continuar funcionando com a fila de dados cheia.

A aprovação da autoridade no protótipo vale para o contexto e o binding mínimo testados. Ao adicionar pedidos de servidor, recursos e apresentação, provar novamente a identidade por instância, as permissões e a rejeição após revogação. Ausência de globais de navegador não valida automaticamente os novos bindings.

Construir uma fatia pequena com entrada, alteração incremental, arraste/desenho e um recurso de mídia gerenciado. Medir no aplicativo nativo sem MOD, com runtimes ociosos, com a fatia ativa e em ciclos de entrar/sair. Registrar toda a árvore de processos, métrica, build e mesma carga na comparação; não comparar a API antiga somente de leitura com a experiência nova como se fossem equivalentes.

Medir memória após aquecimento, CPU ociosa/ativa, latência de interação e de saída, contadores de recursos e impacto na voz. Exercitar loop infinito, inundação da ponte, excesso de heap e recursos e respostas atrasadas. Publicar os limites concretos adotados na bancada e os resultados. Não reutilizar o teto de 8 MiB do servidor como orçamento total do cliente, nem prometer os 100 MB relatados no Windows sem reproduzir a referência.

Windows/WebView2 e Linux/WebKitGTK continuam pendentes até serem testados; a troca para QuickJS não dispensa medir renderer, mídia e ponte nessas plataformas. A entrega seguinte deve mostrar o protótipo, as provas corrigidas, o progresso independente de E2 e uma recomendação de adoção apoiada nas medidas. Só então atualizar ADR, matriz de suporte e status de aceite.

## 6. Revisão da integração em `6cc58c1`

Revisão de código, sem nova execução da bancada. Os números registrados de memória do motor justificam continuar o experimento, mas os itens abaixo precisam ser corrigidos antes de ampliar a fatia visual. A aprovação do protótipo isolado não cobre automaticamente a integração.

### Confirmar parada não é esperar dois segundos

Em `ui/mods-runtime.js`, `executorNativo.encerrou` usa `Promise.race` com um timeout que resolve com sucesso, remove o listener e retorna mesmo sem receber `parou`. `InstanciaDeMod.encerrar` marca `encerrada` inclusive quando a espera falha. No Rust, `mod_nativo_encerrar` remove o executor do mapa antes da confirmação, portanto o contador também pode mostrar zero enquanto ele ainda encerra.

Manter revogação imediata e descarte dos recursos que já podem ser liberados, mas distinguir parada confirmada de prazo excedido. Conservar supervisão e diagnóstico nativos de instâncias em encerramento até a confirmação real. Não converter falha de descarte em contador zero: registrar os recursos cuja liberação falhou. Provar com confirmação atrasada/suprimida que a UI continua responsiva sem anunciar encerramento concluído.

### A instância nativa precisa de identidade completa e revogação própria

Em `src/main.rs`, `mods_nativos` é indexado somente pelo ID textual do MOD; `mod_nativo_encerrar` recebe apenas esse ID. Um pedido antigo pode atingir a nova instância com o mesmo nome. Eventos carregam geração e ID, mas não distinguem substituições da mesma instância dentro de uma geração. `mod_nativo_iniciar` também inicia o código antes de registrar o executor e não faz uma admissão atômica com a revogação. A bomba pode emitir uma mensagem antes de existir um destino registrado para a resposta.

Usar a identidade imutável da instância, geração e pacote em início, entrega, encerramento e eventos. Registrar e admitir o executor antes de liberar a execução; sincronizar isso com a revogação para impedir inserção após a saída. A revogação nativa da sessão deve solicitar diretamente o encerramento dos seus executores, sem depender de a janela chamar `mod_nativo_encerrar`.

Provar início interrompido pela saída, primeira resposta imediata, encerramento atrasado de A após entrada em B com mesmo MOD, substituição na mesma geração e janela sem responder. A tentativa antiga não pode encerrar nem alimentar o novo executor.

### O limite precisa acompanhar os dados até o consumidor

Em `src/executor.rs`, somente a saída de mensagens possui a contabilidade `Fila`; `para_dentro` continua sendo um `mpsc::channel` sem limite, e `entregar` copia/enfileira o JSON sem teto. Na integração, a bomba em `mod_nativo_iniciar` devolve o lugar da saída antes de chamar `janela.emit`: o limite dessa fila não demonstra contenção do que se acumula no transporte/eventos da janela ou dos pedidos assíncronos já admitidos.

Limitar entrada, bytes e trabalho pendente até consumo/admissão real, com confirmação ou controle equivalente. Preservar revogação e confirmação de parada quando houver saturação. Testar consumidor de interface lento/parado, inundação de mensagens diretas e respostas acumuladas, não apenas o canal do protótipo sem consumidor. Não confiar no limite de oito pedidos do prelúdio: o autor pode chamar `seele.postar` diretamente.

### Completar o ciclo dos temporizadores

No ramo de temporizadores vencidos de `rodar`, há `continue` depois de executar callbacks e jobs, antes de `recolher_pedidos_de_relogio`. Assim, um callback que cria outro timeout ou cancela seu intervalo deixa pedidos sem coleta até chegar uma mensagem externa. Processar os pedidos também nesse caminho. Testar timeout que agenda timeout e intervalo que se cancela, sem tráfego externo que esconda o defeito; confirmar que a tabela nativa fica vazia.

Validar também os atrasos antes de convertê-los: um número finito enorme, como `1e300`, pode alcançar `Duration::from_secs_f64` e provocar panic por estar fora do intervalo representável; `Instant + Duration` também precisa de aritmética verificada. Usar uma política explícita de recusa ou limite, sem panic e sem silêncio. Quando os 256 temporizadores forem atingidos, a recusa precisa chegar ao chamador: hoje o anfitrião simplesmente ignora o pedido, mas a fachada conserva o callback.

### Preservar o alcance das medidas

Os valores de `2605c95` são RSS do processo de teste e tempos de uma coleta, sem a bomba, a WebView e a mídia da integração. Zero centésimos de CPU em um segundo significa nenhum incremento observável nessa resolução; não prova consumo absolutamente nulo. Falha na leitura de `ps` deve ser marcada como medição inválida, não convertida em zero. Repetir a coleta integrada e registrar dispersão, carga e confirmação de descarte antes de concluir sobre custo do produto ou impacto na voz.

Após essas correções e provas, continuar a fatia pequena já autorizada: entrada, atualização incremental, arraste/desenho e mídia gerenciada. Não é necessário recomeçar o executor nem interromper o projeto; é necessário fechar os caminhos de ciclo de vida que a integração acrescentou.

## 7. Pendências reproduzidas após `fd1de3a`

A revisão confirma mudanças em identidade nativa, filas de entrada e temporizadores, mas três caminhos da integração continuam abertos. Dois foram reproduzidos executando o código real de `ui/mods-runtime.js` com transporte simulado no Node; o terceiro foi conferido no código da ponte e no Tauri local. Não foi repetida a bancada no aplicativo nesta revisão.

### Inicialização ainda aceita identidade antiga

Enquanto `numero` é `null`, o listener aceita eventos pelo nome do MOD e pela geração. Uma instância anterior do mesmo MOD pode emitir nessa janela. A reprodução entrega uma mensagem e um `parou` da instância 99 antes de o início devolver 100: a mensagem é aceita, sua resposta é perdida porque `entregar` retorna com `numero === null`, e o encerramento de 100 é confirmado pelo `parou` de 99.

Não aceitar fallback por nome. Reservar a identidade nativa e devolvê-la à janela antes de liberar a execução, com ativação em uma segunda etapa ou mecanismo equivalente que preserve ordem e limite de memória. Conferir revogação também na ativação. Provar evento antigo, resposta imediata e pedido de encerramento enquanto a montagem espera a identidade. Não corrigir apenas ignorando todos os eventos iniciais: isso perderia as primeiras mensagens legítimas.

### Confirmação tardia não conclui a limpeza

Quando o prazo vence, `encerrou` retorna falso e mantém o listener. Se `parou` chega depois, ele apenas resolve a promessa antiga: não remove o listener nem atualiza `InstanciaDeMod`. A promessa de `encerrar` já concluída impede nova tentativa. Na reprodução, após a confirmação verdadeira o estado continua `encerrando`, nenhum listener foi removido e chamar `encerrar` novamente devolve falso.

Separar o prazo de espera da conclusão definitiva. A confirmação tardia deve sempre liberar o listener, atualizar a supervisão e concluir o estado quando os recursos tiverem saído, sem readmitir efeitos. Manter registro das instâncias em encerramento fora do mapa de instâncias ativas até a resolução; remover do mapa ativo não pode tornar falhas invisíveis.

### Emitir ainda não é consumir

A bomba agora libera os créditos depois de `janela.emit`, mas não há confirmação da WebView. No Tauri 2.11.5 presente no checkout, `webview::emit_js` chama `eval`, que despacha `eval_script`; esse retorno não representa conclusão do handler JavaScript. O contador `naFila` só aumenta quando o listener já começou a processar o evento. Com a janela parada, ele não limita o que espera antes dele.

Manter créditos nativos de quantidade e bytes até confirmação de consumo/descarte vinculada à instância e à mensagem, ou usar uma fila nativa limitada que a janela drene. O caminho de parada deve ser independente do crédito de dados e continuar sendo confirmado no supervisor nativo. Testar a ponte com entrega à janela suspensa: o produtor deve atingir o teto sem crescimento contínuo antes de qualquer callback JS rodar. A medição nativa com janela ocupada continua necessária para complementar esse teste determinístico.

Reprodução dos dois primeiros pontos: [script de evidência](../evidencias/revisao-mods-fd1de3a.cjs), executado da raiz com `node docs/evidencias/revisao-mods-fd1de3a.cjs`. Em `fd1de3a`, deve retornar código 1 pelos comportamentos acima. É uma reprodução da camada JS com ordem de eventos controlada, não uma medição de memória ou um teste do IPC nativo. Incorporar casos equivalentes à suíte do produto ao corrigir.

## 8. Revisão nativa da colheita em `8adebb8`

Executado `node apps/seele-app/bancada/ciclo-do-executor.cjs`: passou. A API antiga do script de evidência da seção 7 foi substituída; ele permanece como registro histórico, não como teste do novo transporte. A colheita resolve o retorno prematuro dos créditos das mensagens comuns, mas o aceite precisa incluir os caminhos nativos abaixo.

### Descarte de mensagens não pode depender da geração que já saiu

`ModsNativos::confirmar_parada` conserva a instância em `encerrando` enquanto existem `pendentes`. Porém, `mod_nativo_colher` recusa a geração anterior depois de `Session::revogar`. Portanto, sair com uma mensagem ainda na fila deixa dados e a instância retidos mesmo depois de `Parou`, sem caminho de colheita válido. Uma janela que fechou também não deve ser necessária para liberar esses dados.

Ao revogar, descartar nativamente os efeitos não consumidos da geração e acertar seus créditos; mensagens recebidas pela bomba após a revogação também devem ser descartadas. Não entregar efeitos antigos só para esvaziar a fila. Se houver diagnóstico a preservar, copiá-lo para um registro limitado independente do executor. Remover os recursos da instância após a confirmação real, sem aguardar a janela.

Incluir nesse descarte `codigos_reservados`: atualmente a tabela só perde uma entrada na ativação. Reservar e sair sem ativar pode conservar o fonte. A reserva precisa participar do mesmo grupo de descarte, inclusive nas corridas de inserção e falhas de montagem.

Teste necessário: reservar/ativar, produzir mensagens, suspender a colheita, revogar a sessão, confirmar parada e conferir tabelas, bytes, créditos e código reservado vazios. Repetir com reserva sem ativação e sem callbacks da janela. Não basta provar que a fila parou de crescer: ela precisa ser liberada.

### Parada espontânea precisa mudar o estado nativo

`confirmar_parada` escreve `parou = true` somente na tabela `encerrando`. Para a tabela `vivos`, ele verifica `i.parou` sem tê-lo alterado. Assim, uma confirmação de parada de uma instância ainda viva não a remove nem a marca como parada. Unificar o tratamento da confirmação para todos os estados e impedir que um executor terminado continue sendo descrito como ativo.

Para conferir esses dois pontos, o corpo atual de `confirmar_parada` foi compilado isoladamente com estruturas mínimas equivalentes. Resultados: parada confirmada com pendência permaneceu retida; parada espontânea sem pendências permaneceu em `vivos` com `parou=false`. Isso exercita o método de supervisão, não substitui o teste integrado de revogação/colheita pedido acima.

### Erros e interrupções também precisam de contabilidade

`bombear` insere `Falhou` e `Interrompido` na mesma fila `pendentes` das mensagens. Esses avisos são enviados pelo executor sem reservar crédito em `Fila`, mas `mod_nativo_colher` subtrai quantidade e bytes de **todas** as falas colhidas. Colher um erro sem crédito reservado pode fazer o contador atômico dar a volta; repetir erros também contorna o teto da fila de mensagens.

Definir quais classes reservam e devolvem crédito, de maneira simétrica. Aplicar limite ou agregação aos diagnósticos repetidos; o controle terminal de parada deve continuar separado e independente da fila de dados. Provar erro sem nenhuma mensagem, mensagens intercaladas com erros, interrupções repetidas com janela parada e encerramento sob saturação. Os contadores precisam permanecer entre zero e seus tetos, e uma falha do MOD não pode bloquear permanentemente mensagens futuras por contabilidade inválida.

Depois desses testes nativos, continuar as primitivas da fatia. Não alterar a direção da arquitetura nem repetir toda a investigação do executor: fechar o descarte e a contabilidade do transporte que acabou de mudar.

## 9. Revisão dos caminhos completos em `08ae1ec`

Executados nesta revisão: os oito testes `a_supervisao_dos_mods_nativos` passaram e `cargo xtask check-runtime` passou. As correções dos métodos de supervisão e colheita estão presentes. Ainda faltam duas propriedades nos caminhos que conectam esses métodos; os testes locais dos métodos não exercitam as ordens abaixo.

### Reserva e fonte precisam entrar juntos no grupo de descarte

`mod_nativo_reservar` registra a instância em `mods_nativos`, libera esse cadeado, cria a bomba e só depois insere o fonte em `codigos_reservados`. A revogação pode ocorrer entre as duas inserções: ela identifica a reserva e tenta apagar o fonte, que ainda não foi guardado. O comando de reserva então insere o fonte depois do descarte. Se não houver ativação posterior, ele fica retido.

Guardar o fonte junto da instância sob o mesmo domínio de sincronização, ou tornar a inserção posterior uma admissão novamente validada e atômica com a revogação. Tratar falha ao criar a bomba e cancelamento da reserva antes de ativar com rollback completo. Não depender de uma ativação futura para limpar a reserva interrompida.

O teste atual de reserva só confere que seu número aparece em `reservas_da_geracao`; ele não insere o fonte, chama `Session::revogar` ou intercala as duas inserções. Acrescentar uma prova com barreira entre registrar a instância e guardar o fonte: revogar nesse ponto, liberar a barreira e verificar ausência de código órfão e término supervisionado. Exercitar o mesmo caminho usado pelo comando, extraindo a coordenação se necessário.

### O teto dos avisos precisa abranger o transporte e o descarte

`anotar_aviso` limita a lista a 16 e agrega repetições. Porém, `bombear` continua chamando `janela.emit` para cada `Falhou`/`Interrompido`, mesmo quando o aviso foi agregado ou descartado. Um aviso sem corpo ainda ocupa memória como evento. O teste de dez mil erros verifica a lista final, mas não o canal executor→bomba nem o número de eventos esperando na WebView. Os envios de `Falhou`/`Interrompido` no executor também continuam fora da cota de mensagens.

Usar sinalização agregada de dados disponíveis, com no máximo uma notificação pendente por instância, e um caminho limitado/agregado para diagnósticos desde sua produção. Rearmar a sinalização ao colher sem perder a transição vazio→não vazio. Preservar o aviso terminal de parada independentemente das cotas. Testar produtor de erros real com bomba/consumidor lentos e entrega à janela suspensa, contando todos os pontos de retenção e notificações.

Além disso, os ramos de `bombear` para geração revogada ou instância ausente ainda chamam `fila.tirar(corpo.len())` para qualquer fala, inclusive erros que não reservaram crédito. Devolver crédito somente para a classe que o tomou, também nos descartes. Provar um erro tardio intercalado com mensagens ainda contabilizadas, em vez de testar apenas contadores já zerados.

Essas são pendências da integração já em escopo, não novas famílias de funcionalidade. Corrigi-las e provar os caminhos completos antes de declarar a limpeza concluída; depois continuar a fatia experimental autorizada. As medições de plataforma e desempenho integrado permanecem etapas separadas.

## 10. Avanço à fatia após `5b7a694`

Conferidos os caminhos de reserva/fonte, revogação, cotas de avisos, notificação agregada e devolução de créditos. Executados nesta revisão:

- `cargo test -p seele-app --bin seele-app a_supervisao_dos_mods_nativos -- --nocapture`: 13 testes passaram.
- `cargo xtask check-runtime`: quatro casos passaram.

As pendências específicas da seção 9 estão tratadas no código e nesses testes. **Continuar a fatia experimental.** Este avanço não equivale ao aceite final de E1/E2, à aprovação para publicação ou à repetição das medições nativas; não foi executado o aplicativo nesta revisão.

Entregar uma única experiência pequena que combine campo editável e gravação autorizada no servidor, atualização incremental sem perder foco, arraste/desenho e um recurso de mídia gerenciado. Cada operação nova entra com identidade, validação, limite e descarte; não adiar o registro de recursos para depois das primitivas. Manter o controle confiável de saída acessível.

Exercitar essa experiência no aplicativo nativo com saída durante carregamento, reprodução e resposta atrasada, além de troca A→B. Aproveitar a próxima execução para confirmar a integração após o refactor e colher a referência de consumo sem MOD e com o executor ocioso; depois comparar a fatia ativa na mesma carga e métrica. Registrar memória, CPU, latência de interação/encerramento, recursos remanescentes e impacto na voz. Windows e Linux permanecem pendentes até medição real.

Não iniciar ainda a migração completa dos três MODs nem ampliar para uma biblioteca extensa. A próxima entrega é a fatia funcional e sua medição, que sustentará a escolha definitiva do executor e o aceite integrado.

## 11. Orientação de conclusão — prioridade do responsável

O responsável pediu finalizar o desenvolvimento da API. Esta seção consolida o modo de execução a partir de agora: não iniciar outra rodada geral de revisão da infraestrutura antes de implementar a fatia. As exigências funcionais, de isolamento e limpeza do contrato continuam valendo; o trabalho deve avançar para a entrega completa.

### Sequência de entrega, sem novos pedidos de autorização por etapa

1. Terminar a fatia interativa e medir no aplicativo nativo. Se os critérios já definidos passarem, registrar a adoção de QuickJS e prosseguir automaticamente para as próximas entregas. Não parar apenas para perguntar se pode continuar. Se houver falha material, corrigir o caso e repetir a verificação afetada.
2. Completar o contrato implementável da API: composição visual incremental, interação, desenho, tema, recursos, mídia, dados/eventos, permissões, limites, erros e ciclo de vida. Recuperar a matriz funcional de MESA, PERFIS e ESTILO. Comportamentos ausentes não podem ser substituídos por telas de leitura ou avisos de indisponibilidade.
3. Atualizar versão/compatibilidade, guia do catálogo/site, exemplos e os três pacotes de MOD. Manter os pacotes históricos intactos. Documentar o que está implementado; distinguir extensões futuras já identificadas, como 3D, sem aumentá-las ao escopo de encerramento.
4. Executar a validação final abaixo, corrigir falhas materiais e entregar os artefatos prontos para revisão/publicação. A autorização de desenvolvimento não publica releases automaticamente.

### Cinco blocos finais de validação

Não existe um número antecipável de casos individuais: dependem do código que falta. O plano contém cinco blocos finitos, reutilizando os testes existentes:

| Bloco | Critério de conclusão |
| --- | --- |
| Funcionalidade | Matriz dos três MODs recuperada; interação/desenho/mídia reais e experiência independente com as mesmas primitivas |
| Isolamento e limpeza | Dados/permissões de servidor respeitados; saída, troca A→B, respostas/carregamentos atrasados e dois participantes sem efeitos cruzados ou recursos antigos ativos |
| Contenção | Loop, excesso de mensagens/árvores/recursos e falhas do MOD não impedem revogação e recuperação; novos bindings conservam a autoridade da instância |
| Leveza | Comparação nativa em carga equivalente, com memória, CPU, responsividade, voz e ciclos de entrada/saída; sem substituir medida por estimativa |
| Entrega e compatibilidade | Versões, exemplos, MODs e guia concordam; verificações exigidas pelo repositório passam; resultados reais da matriz de plataformas registrados |

Windows e Linux não podem ser marcados como aprovados sem execução. A indisponibilidade dessas máquinas não impede concluir implementação e documentação independentes; a entrega deve separar desenvolvimento concluído de homologação pendente, sem anunciar suporte validado que não foi medido.

### Limite das rodadas de revisão

Criar testes ao implementar comportamentos e ao corrigir defeitos concretos. Reutilizar os guardas existentes, executar as verificações exigidas e repetir apenas o que novas mudanças ou falhas justificarem. Não exigir uma nova campanha de reversões para toda função, nem reabrir arquitetura ou repetir revisões gerais a cada relato de progresso.

Bloqueiam a entrega: função contratada ausente, acesso indevido entre instâncias/servidores, execução/recurso que persiste indevidamente após saída, falha de responsividade/recuperação, regressão relevante de consumo e inconsistência de versão/compatibilidade. Melhorias de organização, comentários e hipóteses sem defeito demonstrado não devem transformar este fechamento em uma expansão indefinida do projeto.

## 12. Continuação após a fatia de `d0f3072`

O relato nativo registra intermitência sem causa comprovada; uma execução bem-sucedida não encerra esse defeito. Nesta revisão foi reproduzida e corrigida uma perda de sinalização em `ui/mods-runtime.js`, sem alterar o transporte ou a arquitetura:

1. O Rust atende uma colheita vazia; a resposta ainda está em trânsito para o JavaScript.
2. Uma mensagem nova entra e produz seu aviso. A janela o recebe com `colhendo = true` e antes o ignorava.
3. A resposta vazia antiga chega; o coletor sai e deixa a mensagem nova retida, sem outro aviso para buscá-la.

Reprodução anterior à correção: uma colheita, zero mensagens entregues e uma mensagem retida, com `colhendo = false`. O caso entrou na bancada existente como `avisoDuranteColheitaVaziaNaoSePerde` e reprovou antes do conserto. Agora uma marca de nova colheita preserva o aviso recebido durante a espera, sem permitir consultas concorrentes nem criar polling ocioso. As cinco corridas do executor e as oito provas da região passam por `cargo xtask check-runtime`.

Essa é uma falha demonstrada na ordem dos eventos da camada JS. **Ainda não prova que era a causa das execuções nativas anteriormente registradas.** O próximo passo é recompilar o aplicativo que embute a interface e repetir o caso nativo antes de considerar a intermitência resolvida. A correção está na árvore de trabalho para o Claude continuar; não sobrescrever os arquivos com uma cópia anterior.

Na verificação de interface apareceu também um guarda textual desatualizado: ele exigia `if (!minhaInstancia()) return;`, embora `base.js` já registrasse o motivo dentro de um bloco. O guarda agora confere o retorno no bloco, preservando a propriedade sem proibir o diagnóstico.

Depois da repetição nativa, retomar diretamente a sequência de conclusão da seção 11. A chave de produção pertence à assinatura/publicação do catálogo: não é pré-requisito para implementar a versão da API, preparar os artefatos, o guia e os MODs e testar com fixtures/pacotes locais e chaves de teste em ambiente isolado. Não alterar a confiança de produção nem substituir pacotes históricos para contornar essa separação.

A recusa de acessibilidade do `osascript` deve ser registrada como impedimento daquele método de automação. Ela não impede continuar implementação e documentação; a interação nativa restante pode ser exercitada por uma ferramenta autorizada ou por homologação manual. Não confundir montagem de elementos e carregamento de mídia com comprovação de digitação, arraste, reprodução audível e desempenho sob voz.

## 13. Fechamento da implementação após `e16011b`

O relato registra seis inicializações nativas completas e a comparação da fatia montada: 246,7 → 267,9 MiB e 6,0% → 6,1% de um núcleo. São resultados para aquela carga, sem interação, e não uma medida dos três MODs completos com voz. Não reabrir a investigação geral da colheita sem nova falha. O trabalho restante é concluir o produto conforme o escopo contratado.

**Ainda não é correto declarar que só falta a chave do responsável.** Conferência do checkout:

| Entrega | Estado observado | Fechamento necessário |
| --- | --- | --- |
| Executor padrão | `executor_de_mods` retorna Worker salvo `SEELE_EXECUTOR=quickjs`; os comandos nativos dependem dessa variável | Tornar QuickJS o caminho normal da versão nova; retirar a dependência de configuração de bancada e o fallback de produção para o Worker já reprovado |
| Versão integrada | `MOD_API_VERSION` permanece 2; os MODs foram exercitados num build com alteração temporária | Preparar uma alteração versionada e reproduzível de API 3, com contrato, pacotes, testes e plano de compatibilidade; não depender de um bump manual temporário para demonstrar a entrega |
| Funções contratadas | O ADR 0049 declara tipografia e seleção de arquivos ausentes; o guia ainda limita os MODs à região própria | Completar as funções previstas no contrato e a matriz anterior dos três MODs, inclusive fontes/aparência, seleção e envio de imagens e integração nas superfícies autorizadas |
| Guia coerente | Há referência a botões/mídia, mas os passos iniciais ainda dizem que o contador só lê e que entrada/imagem/som exigem extensão; a execução ainda é descrita como Worker de Blob | Revisar a fonte inteira e regenerar exemplo/site de acordo com o executor e capacidades finais |

A seleção de arquivo já foi prevista como ação do usuário mediada pelo SEELE, com handles e cancelamento. Isso não dá ao MOD acesso arbitrário ao disco. A tipografia já foi prevista como recurso de sessão, registrado e removido ao sair, sem alterar preferências globais. Atualizar os ADRs necessários para expressar esse desenho; declarar ausência num ADR novo não reduz automaticamente o escopo autorizado pelo responsável.

Para os três MODs, fechar a matriz de comportamentos antes/depois que o contrato já exige. Mostrar uma imagem antiga não recupera a possibilidade de escolher e enviar uma nova; exibir um tabuleiro e mover peças não comprova toda a gestão de campanhas, fichas e cenas; seis cores não substituem todas as opções anteriores do ESTILO. Marcar cada comportamento como implementado e exercitado ou como pendente concreto. Não tratar a remoção de um aviso textual de indisponibilidade como recuperação da função.

### Desenvolvimento e publicação são entregas separadas

Preservar os vetores oficiais assinados existentes como históricos. Preparar a nova versão e seus testes com fixtures e chaves de teste isoladas, sem substituir a confiança de produção, desativar verificação de assinatura ou publicar pacotes prematuramente. A comparação com o catálogo atualmente publicado precisa continuar testando o que ele representa, sem obrigar que a única prova da versão em desenvolvimento seja um catálogo já publicado.

Entregar juntos, para revisão: alteração definitiva da versão/executor, guia, exemplos, pacotes candidatos e sequência de migração de clientes, servidores e catálogo. A sequência precisa preservar a coerência entre versões; publicar primeiro o catálogo novo não garante por si só que clientes antigos e servidores já instalados continuem funcionando. A assinatura final e a publicação são o passo do responsável quando os artefatos estiverem prontos; não são justificativa para deixar a implementação em modo de bancada.

Continuar segundo os cinco blocos finitos da seção 11, verificando as mudanças realizadas e reaproveitando as suítes existentes. Não abrir outra campanha genérica de testes. Homologação nativa de interação, Windows/Linux e carga de voz permanece explicitamente separada do desenvolvimento, com o que foi executado e o que depende de ambiente disponível.

### Conferência seguinte: `6b37263`

**Executor padrão e versão integrada estão entregues no checkout.** `MOD_API_VERSION` vale 3, `api/v3.json` existe, e a seleção de executor de bancada/Worker foi removida do caminho executável. Não reabrir esses trabalhos sem falha concreta. Esta conferência leu código e documentos; não repetiu o aplicativo nem a bateria completa que o relato ainda acompanhava.

A matriz `matriz-dos-tres-mods.md` ainda contém **seis grupos funcionais pendentes**, não apenas os quatro citados para MESA:

1. ESTILO: arredondamento e brilho, já existentes na experiência anterior.
2. PERFIS: cartão integrado à lista de pessoas.
3. MESA: magias preparadas e espaços por nível.
4. MESA: ações com fórmula, usos e recuperação.
5. MESA: editar/publicar verbetes existentes do compêndio.
6. MESA: redimensionamento da grade, descrição e notas do GM da cena.

Completar esses grupos com o desenho já autorizado. Para ESTILO, definir propriedades/tokens validados e confinados à sessão. Para PERFIS, oferecer um ponto de integração controlado pelo renderer do SEELE: o MOD declara o cartão e o produto o monta. Isso não coloca JavaScript do MOD na janela nem exige seletores livres do DOM. Atualizar os ADRs que ainda apresentam essas funções como impossíveis pelo isolamento; execução e superfície de apresentação são fronteiras distintas neste contrato.

O guia do indexador ainda precisa de uma última correção de conteúdo: diz que o contador só envia `ler` e não tem incremento; diz que o seletor existirá no futuro; sua tabela de formas não inclui `arquivo`, embora o contrato v3 já o inclua. Conferir também a seção de tipografia com a implementação atual, regenerar a saída e o exemplo. A alteração deve corrigir o conteúdo inteiro, não apenas trocar a palavra Worker.

O plano `migracao-para-api-3.md` promete chegar às pessoas sem quebrar quem está no ar, mas reconhece que a atualização para API 3 recusa pacotes API 2 e deixa uma janela sem pacotes compatíveis. Ajustar essa promessa e planejar a atualização coordenada do aplicativo, dos MODs exigidos por cada servidor e do catálogo; não apresentar a troca como transparente. Preparar os artefatos antes da publicação e preservar dados/pacotes históricos. O processo de assinatura continua separado do desenvolvimento.

Esta é a lista restante de implementação/documentação da seção 13. Concluir os seis grupos, corrigir guia e plano de migração, registrar o término da bateria em andamento e entregar. Reaproveitar as verificações existentes e acrescentar somente a cobertura dos comportamentos implementados. Homologação nas plataformas e interação/voz permanece nomeada separadamente; não anunciar a restauração integral dos três MODs enquanto a matriz ainda contiver estas funções anteriores ausentes.

### Conferência de escopo: `4201857`

O relato final informa a bateria concluída e os quatro grupos da MESA implementados. Não se pede nova campanha de testes nem investigação do executor. Duas linhas, porém, foram encerradas substituindo o comportamento contratado:

- ESTILO: arredondamento e brilho passaram de pendentes para recusados, com `RECUSADOS_POR_DESENHO` em `base.js`. Recusar explicitamente melhora o diagnóstico, mas não implementa a função que já existia e foi pedida de volta.
- PERFIS: o cartão da lista passou a ser uma marca de texto de até 24 caracteres com cor de contorno, usada para pronome. Essa marca é útil, mas não demonstra a recuperação do cartão de perfil anterior. A matriz mudou o nome da linha; isso não encerra a comparação funcional.

**A orientação do responsável é MODs livres, leves, restritos ao servidor e limpos ao sair.** Preservar o visual padrão do SEELE não autoriza reduzir essas funções sem decisão dele. As instruções desta migração já preveem tema de sessão e superfícies controladas pelo renderer. Ajustar os documentos de design/ADRs para distinguir a identidade padrão do produto das personalizações de servidor autorizadas, preservando símbolos institucionais e controles confiáveis de saída/recuperação.

Fechamento de ESTILO: implementar os valores anteriores de `radius` e `glow` por propriedades validadas e limitadas, aplicadas somente aos alvos da sessão. Restaurar a edição, gravação, aplicação e remoção ao sair. Não oferecer CSS arbitrário nem alterar o tema pessoal fora daquela sessão. Registrar a exceção de tema de servidor nas regras de design necessárias; não encerrar novamente como recusa por marca.

Fechamento de PERFIS: comparar o cartão anterior no histórico com o que é apresentado agora e recuperar seu conteúdo/aparência funcional no ponto de integração da lista. Somente o renderer do SEELE cria os elementos, a partir de dados e recursos autorizados do MOD. A marca de pronome pode permanecer como parte do cartão; não equivale, sozinha, à migração do cartão. Preservar os controles nativos de pessoa, voz e moderação.

Concluir esses dois comportamentos, corrigir a matriz e o guia e executar apenas as verificações afetadas. O escopo não está sendo ampliado: são os dois requisitos já nomeados na seção 13 que foram substituídos. Homologação restante e assinatura/publicação continuam separadas. Não relatar recuperação integral enquanto a função original estiver apenas preservada no banco, recusada ou substituída por um subconjunto não acordado.

### Fechamento dessas pendências em `998b0f0`

Conferidos no código: `arredondamento` e `brilho` são propriedades aplicáveis do tema e são usados pelo ESTILO; `SeeleUI.cartoes` integra declarações do PERFIS ao renderer da lista, com mídia, tetos e descarte. O contrato v3 e a matriz refletem esses comportamentos. As duas substituições de escopo apontadas acima foram corrigidas.

**Encerrar esta etapa de desenvolvimento e seguir à homologação e à preparação da publicação.** Não reabrir o executor, a arquitetura ou uma campanha geral de testes sem falha nova. As verificações afetadas foram informadas como verdes no relato; nesta conferência foram lidos código e documentos, sem repetir as suítes ou o aplicativo.

Continuam separados: interação nativa real (digitação, arraste, reprodução e saída durante uso), carga de voz/desempenho integrado e Windows/Linux; depois, revisão dos artefatos e execução autorizada da migração/publicação. Não apresentar esses itens como já homologados nem confundir o encerramento do desenvolvimento com uma publicação realizada.
