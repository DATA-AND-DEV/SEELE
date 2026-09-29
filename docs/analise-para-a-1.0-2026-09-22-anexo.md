# Anexo da análise para a 1.0: as nove frentes, na íntegra

Gerado em 22/09/2026 a partir do HEAD `e2fac4d`, que é a v0.15.0 publicada. Cada frente foi lida por um analista e depois revista por um cético, que tentou refutar as afirmações de carga dele.

O relatório principal é `docs/analise-para-a-1.0-2026-09-22.md`. **A severidade que vale é a do relatório principal.** Aqui fica a do analista, e ao lado dela a contestação do revisor, quando houve.

Legenda das evidências: **MEDIDO** (rodado ou observado), **LIDO** (está no código) e **INFERIDO** (dedução).


---

## Persistência de servidores e do link

**Resumo do analista.** Quase toda a cadeia que faz um link sobreviver já está construída e publicada na v0.15.0 (e2fac4d): impressão digital persistente no banco, porta 8383 fixa, UPnP que tenta 8383 antes de aceitar outra, histórico que guarda fp, caminhos e bilhete, e o quarto (MORO/QUEM), que já responde em produção. Falta um elo, e ele está quebrado desde que o quarto nasceu (ce4875b, 03/09). A escuta de avisos do anfitrião se registra com a marca fixa `anfitriao` (alcance/encontro.rs:447 e :816), enquanto o convidado pergunta pela impressão digital (seele-ffi/src/lib.rs:7805-7815). Por isso o quarto devolve onde o servidor está, mas nunca onde pedir o furo. No caso comum (CGNAT, ou NAT sem UPnP) o link antigo continua morto depois de o anfitrião reiniciar, e o próprio produto admite isso em frases.js:770. O conserto mexe só no anfitrião: leva horas, não sobe protocolo e funciona com todo cliente 0.15.0. É o único bloqueante da frente A. Com ele, o que o ADR 0047 dá como pronto passa a ser verdade. Na frente B, o servidor hoje vive o tempo da janela e da sessão do anfitrião: SAIR, trocar de servidor e fechar a janela derrubam o servidor sem perguntar. Não há bandeja, nem iniciar com o sistema, nem impedir o sono. O seeled vem no pacote, mas o app nunca o abre e ele não sobe a escada. Para a 1.0 recomendo o mínimo: não derrubar sem perguntar, manter no ar ao sair, impedir o sono e deixar a bandeja e o seeled como serviço para a 1.1.

### O que existe

- `construido_e_publicado`: Identidade TLS persistente no banco (fp estável entre reinícios) (crates/seele-server/src/tls.rs:66-88; lib.rs:516; app com Location::File em apps/seele-app/src/main.rs:1562)
- `construido_e_publicado`: Uma chave por máquina para todos os servidores guardados (apps/seele-app/src/servidores.rs:233-297)
- `construido_e_publicado`: Porta de escuta fixa 8383 no app (apps/seele-app/src/main.rs:1561, :1963)
- `construido_e_publicado`: UPnP tenta 8383 de novo antes do recuo, e avisa quando a porta não é a canônica (crates/seele-server/src/alcance/porta.rs:425-490 (503b1db); alcance.rs:960)
- `construido_e_publicado`: Link com alt/enc/fp/convite/v (crates/seele-proto/src/uri.rs:81-124, :358-399; seele-server/src/hospedagem.rs:198-232)
- `parcial`: Link com nome público (DNS) como alvo (crates/seele-server/src/hospedagem.rs:270-292 (c42a30f); uri.rs:1171-1196)
- `construido_e_publicado`: Histórico guardando caminhos, bilhete e impressão (crates/seele-core/src/conhecidos.rs:41-107, :241-270; gravado em apps/seele-app/src/main.rs:1248-1272)
- `construido_e_publicado`: Quarto (MORO/QUEM) no ponto de encontro, em produção (crates/seele-encontro/src/lib.rs:73-150, :284-295; sondado pelo coordenador em 22/09)
- `construido_e_publicado`: Registro do socket do servidor no quarto (fp16+s) (crates/seele-server/src/alcance/encontro.rs:818-820)
- `parcial`: Registro da escuta de avisos no quarto (crates/seele-server/src/alcance/encontro.rs:447, :816 — registra como «anfitriao», não pela impressão)
- `construido_e_publicado`: Consulta ao quarto pelo convidado antes de conectar (crates/seele-ffi/src/lib.rs:7780-7815; apps/seele-app/src/main.rs:1006-1030)
- `ausente`: Teste de ponta a ponta anfitrião ↔ quarto ↔ convidado (crates/seele-conformance/tests/quarto.rs usa MORO feito à mão (:51, :62))
- `construido_e_publicado`: Portaria que lembra quem já foi admitido (membro volta sem convite) (crates/seele-server/src/session.rs:640-676; portaria.rs:378-392)
- `construido_e_publicado`: Versão no link e abrir a versão do servidor ao colar (apps/seele-app/src/main.rs:6929-6947; ui/camada-versoes.js; ui/camada-servidores.js:187-190)
- `construido_mas_nao_ligado`: Semeadura de dados/identidade entre versões (crates/seele-lancador/src/dados.rs (plano_de_dados/aplicar sem chamador no app))
- `construido_mas_nao_ligado`: seeled embarcado no pacote do app (apps/seele-app/tauri.release.conf.json (externalBin); nunca lançado pelo app)
- `construido_e_publicado`: Argumento --hospedar/--nome-publico na abertura (base para autostart) (apps/seele-app/src/main.rs:278-292; ui/tela-boot.js:590-594)
- `ausente`: Bandeja/segundo plano/iniciar com o sistema (apps/seele-app/tauri.conf.json e Cargo.toml — nada)
- `ausente`: Impedir o sono enquanto hospeda (grep por IOPMAssertion/SetThreadExecutionState/inhibit: nada)
- `ausente`: seeled como serviço (systemd/launchd/Windows) e apontado para o banco do app (só existe unidade para o seele-encontro: docs/ponto-de-encontro.md:106)
- `so_desenho`: Multiconexão / hospedar sem estar dentro (apps/seele-app/src/main.rs:899 (AlreadyConnected); :2005-2025 (SAIR derruba a hospedagem); ADR 0031 proposto)

### Lacunas

- **[BLOQUEANTE · esforço P · risco baixo] O quarto nunca devolve a escuta de avisos: o anfitrião se registra como «anfitriao» e o convidado pergunta pela impressão digital.** A cada 15 s, `atender` manda `MORO anfitriao` pela escuta de avisos. A marca é fixa e igual para todo anfitrião do mundo. Pelo socket do servidor vai `MORO <fp16>s`. O convidado pergunta `QUEM <fp16>s` e `QUEM <fp16>`, e ninguém registra `<fp16>`: `fresco_do_aviso` é sempre None, `connect` mantém o aviso de ontem e o LEVE cai num mapeamento morto. O anfitrião não fura, e o Initial do QUIC é descartado por NAT que filtra por endereço de origem (o doméstico comum e a maior parte do CGNAT). É exatamente o caso que o quarto existe para cobrir: voltar pela trilha, ou colar um link antigo, depois de o host reiniciar. O defeito nasceu em ce4875b, nos dois lados ao mesmo tempo, e está em toda release desde a v0.10.2. Nenhum guarda pega: `quarto.rs` faz o MORO à mão, o `sondar` usa a marca `sondagem`, e o CI não roda. Efeito colateral: a vaga `anfitriao` pertence a um anfitrião qualquer e entrega o endereço dele a quem perguntar. O conserto é só no anfitrião e não sobe protocolo, porque os clientes 0.15.0 já perguntam pela marca certa. (a) MORO da escuta de avisos com `de_quem_chega` (fp16). (b) Filtrar o eco do próprio MORO: a resposta vem como `AQUI <fp16> <meu endereço>` e passaria no filtro de marca do `atender`, virando um furo contra si mesmo; resolve-se mandando um `ONDE` no mesmo tique e ignorando o AQUI cujo endereço é o próprio reflexo. (c) Primeiro MORO na subida, e não 15 s depois. (d) Teste de ponta a ponta em seele-conformance: `alcance::encontro::abrir` contra um `seele_encontro::Ponto` com Quarto, e depois `seele_ffi::onde_mora_hoje` devolvendo os dois endereços, o do aviso igual ao do bilhete. Ele tem de ficar vermelho ao reverter para `minha_marca`. (e) Medida de campo: host em CGNAT fecha e abre; convidado no 5G volta pela trilha e cola o link antigo.
  - *Evidência:* LIDO: crates/seele-server/src/alcance/encontro.rs:447 (`Marca::nova("anfitriao")`), :523, :786, :816 (`moro(&minha_marca)`), :818-820 (`moro(marca_do_server)`), :791 (primeiro tique consumido); crates/seele-ffi/src/lib.rs:7786-7815 (`do_aviso = Marca::nova(prefixo)`); apps/seele-app/src/main.rs:1006-1030 (`None => Some(bilhete_guardado.clone())` em :1026); seele-encontro/src/lib.rs:284-287 + seele-proto/src/encontro.rs:353-356 (MORO registra e ecoa). MEDIDO: grep de `moro(` → só :816, :819 e testes; `git tag --contains ce4875b` → v0.10.2 em diante. INFERIDO: comportamento de filtragem do NAT.
- **[importante · esforço P · risco baixo] A frase do degrau 4 diz que o link morre ao fechar e, depois do conserto, dirá errado; nenhum degrau diz se o link sobrevive.** Hoje `FuroDeNat` diz a verdade sobre um defeito. Depois do conserto, vai mandar gerar outro link à toa. Os outros degraus não dizem nada sobre persistência, a não ser o aviso de porta não canônica. O produto sabe o que vale para cada um: degrau 1 sem nome morre com o IP; degrau 3 canônico vale enquanto o IP público não mudar; degrau 4 vale pelo quarto enquanto o ponto de encontro estiver no ar. A frase só deve mudar depois da medida de campo.
  - *Evidência:* LIDO: apps/seele-app/ui/frases.js:768-770; alcance.rs:952-962 (porta_nao_canonica); main.rs:1609-1620 (o que a tela recebe).
- **[importante · esforço P · risco baixo] O convidado não distingue «quem hospeda está fora do ar» de «rede ruim».** Com o host desligado, o convidado espera o quarto e todos os candidatos e lê «OS PACOTES SAÍRAM E NADA VOLTOU… Confira o endereço e a porta UDP», o que manda a pessoa mexer em algo que está certo. `QUEM` cala tanto quando o ponto está fora do ar quanto quando o host está, então hoje o produto não consegue separar os dois. Com um `ONDE` em paralelo ao `QUEM`: se o ponto respondeu e o quarto calou, a frase vira «quem hospeda não está no ar agora».
  - *Evidência:* LIDO: apps/seele-app/ui/frases.js:620-623; seele-proto/src/encontro.rs:357-360; seele-ffi/src/lib.rs:7797-7815; seele-encontro/src/lib.rs:73 (prazo de 60 s).
- **[importante · esforço P · risco medio] Emitir um único convite fecha o link simples para toda pessoa nova, sem aviso.** `aceita_convites` conta todos os convites já emitidos, inclusive gastos e vencidos. Depois do primeiro, o servidor deixa de ser aberto para sempre, e o link de HOSPEDAR (sem token) passa a dar CREDENCIAL RECUSADA a quem nunca entrou. A pessoa não vai para a fila. Enquanto isso, a camada da portaria continua dizendo «ligada — quem nunca entrou precisa da sua permissão». É uma forma direta de «o link que mandei deixou de funcionar», e o host não vê nada. Opções: com a portaria ligada, chegar sem segredo vira batida na fila; ou contar só convites válidos; ou, no mínimo, a tela dizer que o link simples deixou de admitir gente nova. É decisão de produto.
  - *Evidência:* LIDO: crates/seele-server/src/admissao.rs:94-107, :153-156; session.rs:533-537, :664-668; apps/seele-app/ui/camada-portaria.js:185-191; link sem token em main.rs:1613.
- **[importante · esforço M · risco medio] A vida do servidor é a da sessão do anfitrião: SAIR, trocar de servidor na trilha e fechar a janela derrubam, sem perguntar.** `disconnect` encerra a hospedagem. Fechar a janela mata o processo e não chama `encerrar`, então a regra de UPnP fica no roteador por até 1 h. Nada pergunta antes, mesmo com gente dentro. O host não consegue visitar o servidor de um amigo sem derrubar o dele. `connect` não recusa por haver hospedagem (só recusa uma segunda conexão), e a portaria é administrada direto no banco, sem passar pelo fio. Por isso «sair e manter o servidor no ar» cabe sem multiconexão.
  - *Evidência:* LIDO: apps/seele-app/src/main.rs:2005-2025, :8110-8150, :899; crates/seele-server/src/alcance/porta.rs:86; hospedagem.rs:294-313; README.md:61-64. MEDIDO: grep de CloseRequested/beforeunload → nada.
- **[importante · esforço M · risco baixo] O sono do computador derruba a sala e pode remapear o NAT sem o anfitrião saber.** Nada impede o sono enquanto se hospeda. Na volta, o mapeamento de NAT pode ter mudado. O link copiado depois disso ainda leva o aviso e o endereço público da abertura, porque `Encontro` fixa os dois. A única pista de remapeamento é o AQUI do próprio reavivamento, e `atender` o descarta. Com o conserto da primeira lacuna o quarto recupera quem volta, mas o host continua sem saber que a sala caiu.
  - *Evidência:* LIDO: crates/seele-server/src/alcance/encontro.rs:405-413, :832-836. MEDIDO: grep de IOPMAssertion/SetThreadExecutionState/inhibit → nada. INFERIDO: remapeamento após o sono (depende do roteador).
- **[importante · esforço G · risco medio] Sem bandeja, sem segundo plano e sem iniciar com o sistema.** Quem quer o servidor no ar a noite toda precisa deixar a janela aberta e o computador acordado. O `--hospedar` já existe e é a peça de um autostart. Falta escolher qual servidor (`--servidor <id>`), um ícone na bandeja (feature `tray-icon` do Tauri, que hoje não está ligada) e o registro de iniciar com o sistema (LaunchAgent, chave Run no Windows, autostart XDG). O Cargo.toml do app tem política estrita de dependências.
  - *Evidência:* LIDO: apps/seele-app/tauri.conf.json (plugins: só o atualizador); apps/seele-app/Cargo.toml (`tauri = { features = [] }`); main.rs:278-292 (--hospedar).
- **[importante · esforço G · risco baixo] O seeled não continua o mesmo servidor do app, e não há instalação como serviço.** O banco é relativo à pasta de onde se chama (`./seele.db`), e rodar de outra pasta dá outra identidade. O seeled não sobe a escada: sem UPnP, sem degrau 4, sem quarto, e o convite sai só com o endereço da rede local. Não semeia a portaria, então membros que entraram por convite não voltam. Vem no pacote do app e nunca é aberto. Não há unidade de serviço documentada para ele. Continuar o mesmo servidor só funciona à mão com `SEELE_DB` apontando para `servidores/<id>/seele.db`, e o link de fora morre.
  - *Evidência:* LIDO: crates/seele-server/src/main.rs:50-68, :169-172, :190-203; hospedagem.rs:95-97; portaria.rs:379-381; apps/seele-app/tauri.release.conf.json; apps/seele-app/src/main.rs:1580-1590 (portaria semeada só pelo app); docs/ponto-de-encontro.md:106. MEDIDO: nenhum `Command::new` de seeled em apps/seele-app/src.
- **[importante · esforço M · risco medio] Trocar a VERSÃO QUE VAI HOSPEDAR troca a identidade do servidor, e todo link anterior é recusado.** A outra versão roda com `SEELE_HOME` próprio: outro seele.db, outro `identidade-servidor.db`, outra chave Ed25519 da pessoa, outros pinos e outro histórico. Na prática é outro servidor, com outro `fp`: quem tem o link da 0.15 é recusado por fp divergente, e quem hospeda vira outra pessoa na portaria dos amigos. A semeadura do seele-lancador já existe e não está ligada. No mínimo, herdar as duas identidades; ou a tela dizer «isto é outro servidor, com outro link».
  - *Evidência:* LIDO: apps/seele-app/src/versoes.rs:218-231; crates/seele-lancador/src/deposito.rs:170-172; apps/seele-app/src/main.rs:848-851; crates/seele-ffi/src/lib.rs:1297 (identity.key no home). MEDIDO: grep de plano_de_dados/aplicar fora do crate → nada.
- **[importante · esforço M · risco medio] O histórico é chaveado pelo endereço de rede local do anfitrião.** O `alvo` de um link é o endereço da casa de quem hospeda (192.168.0.x:8383), e é por ele que o convidado guarda e busca na trilha. Dois amigos com o mesmo endereço local se sobrescrevem: a trilha mostra um só, e a volta usa caminhos, bilhete e impressão do outro. A chave natural é a impressão digital, que já está guardada.
  - *Evidência:* LIDO: crates/seele-core/src/conhecidos.rs:170-171, :179-231; crates/seele-proto/src/uri.rs:84-89; hospedagem.rs:198-206. INFERIDO: frequência da colisão (faixas padrão dos roteadores brasileiros).
- **[importante · esforço M · risco medio] Subir o protocolo quebra todos os links entre versões, e a trilha não sabe a versão do servidor.** A 7→8 fez a 0.14 e a 0.15 não conversarem em nenhuma direção. O link colado sabe abrir a versão certa, se ela estiver instalada; a volta pela trilha não sabe, porque `Conhecido` não guarda versão, e dá `Incompatible`. Para a 1.0, a promessa de que o link sobrevive precisa de uma regra: compatibilidade de protocolo dentro da 1.x, ou a trilha guardando a versão e abrindo-a.
  - *Evidência:* LIDO: empacotar/notas/0.15.0.md:1-14; crates/seele-core/src/conhecidos.rs:41-107 (sem versão); apps/seele-app/src/main.rs:6929-6935; ui/frases.js:32-34.
- **[desejável · esforço P · risco baixo] O nome público não é lembrado entre hospedagens.** Quem tem DNS precisa digitar o nome a cada HOSPEDAR. Se esquecer, o link sai numérico e perde a razão de ter nome. Um campo `nome_publico` com `#[serde(default)]` em `Servidor`, reaplicado ao hospedar, resolve.
  - *Evidência:* LIDO: apps/seele-app/ui/tela-boot.js:428, :547; apps/seele-app/src/servidores.rs:43-63; mensagem do commit c42a30f («nasce vazio»).
- **[desejável · esforço P · risco baixo] `caminhos` cresce a cada volta pelo quarto, sem teto.** Cada endereço novo que o quarto devolve é inserido na frente e gravado de volta no histórico, e o `connect` não corta a lista. Depois do conserto da primeira lacuna isso acontece a cada reinício do host, e o histórico acumula candidatos mortos que entram na corrida. Basta cortar em `LIMITE_DE_ALVOS` ao gravar e não persistir o endereço que veio do quarto.
  - *Evidência:* LIDO: apps/seele-app/src/main.rs:1012-1019, :1067; crates/seele-ffi/src/lib.rs:1290-1294; crates/seele-proto/src/uri.rs:79, :161-176.
- **[desejável · esforço M · risco medio] O link identifica a máquina, não o servidor guardado.** Todos os servidores de uma máquina compartilham a chave e a porta. O link da «Mesa de RPG» leva ao servidor que estiver no ar, e a entrada do histórico do convidado troca de nome e ícone conforme o servidor. Pode ser o comportamento desejado (um endereço por pessoa) ou não; é decisão de produto.
  - *Evidência:* LIDO: apps/seele-app/src/servidores.rs:233-297; apps/seele-app/src/main.rs:1561; crates/seele-core/src/conhecidos.rs:308-340 (anotar_aparencia por alvo).

### O que o revisor conferiu

- **CONFIRMADA**: 1. A escuta de avisos do anfitrião se registra no quarto com a marca fixa «anfitriao», e o convidado pergunta pela marca derivada da impressão digital; por isso o quarto nunca devolve o endereço onde pedir o furo.
  - LIDO: crates/seele-server/src/alcance/encontro.rs:447 (`minha_marca = Marca::nova("anfitriao")`), :816 (`avisos.send_to(&encontro::moro(&minha_marca), ponto)`), :818-820 (só o socket do servidor usa `marca_do_server` = fp16+"s", definida em :374-376). Convidado: crates/seele-ffi/src/lib.rs:7805-7815 (`do_aviso = Marca::nova(prefixo)`, `do_server = prefixo+"s"`). Quarto: crates/seele-encontro/src/lib.rs:284-291 (só `Pedido::Moro` grava; ONDE/LEVE não). MEDIDO: `grep -rn -e 'moro(' crates apps --include='*.rs'` → só encontro.rs:816/:819, sondar.rs:190, seele-encontro/src/lib.rs:497 e quarto.rs (testes). `git log -S` mostra que `anfitriao` vem de ba6a066 (17/08), e o MORO com ela nasceu em ce4875b (03/09); `git tag --contains ce4875b` → v0.10.2 em diante. Agravante que confirma o efeito colateral citado: `Quarto::morar` (seele-encontro/src/lib.rs:113-139) é «quem escreveu primeiro fica» por IP, então a vaga `anfitriao` é de um anfitrião qualquer do mundo e os demais são recusados em silêncio.
- **NAO_VERIFICAVEL**: 2. Sem o aviso fresco, um anfitrião atrás de NAT que filtra por endereço não é alcançado depois de reiniciar, mesmo com o endereço novo do servidor vindo do quarto.
  - LIDO (a parte de código): apps/seele-app/src/main.rs:1026 (`None => Some(bilhete_guardado.clone())`); a escuta de avisos nasce em porta efêmera a cada subida (alcance/encontro.rs:636-643, bind em porta 0), então o `aviso` de ontem está sempre morto. O comportamento do NAT não foi medido por ninguém: é INFERIDO pelo analista e continua inferido aqui. **Correção:** A premissa «mesmo com o endereço novo do servidor vindo do quarto» é falsa em produção: o endereço novo do servidor também não chega, porque o convidado nunca manda o QUEM (ver a omissão do ponto sem porta). O caso real é pior: sob CGNAT (porta pública aleatória), nem o endereço nem o furo se renovam. Num NAT doméstico que preserva a porta, o 8383 fixo (main.rs:1963) pode cair no mesmo endereço público de ontem. Aí tudo depende da filtragem: com filtragem independente do endereço passa, com filtragem dependente do endereço não passa.
- **REFUTADA**: 3. Todo o resto da cadeia já funciona e está publicado (impressão digital estável, histórico com fp/caminhos/bilhete, quarto em produção); o conserto é local e pequeno.
  - Metade confirmada, LIDO: identidade no banco (crates/seele-server/src/tls.rs:66-88, lib.rs:514-519) e uma chave por máquina (apps/seele-app/src/servidores.rs:233-297). O quarto em produção responde (medida do coordenador). O elo do convidado, porém, não funciona. PONTO_PADRAO é "encontro.seele.app.br", **sem porta** (alcance/encontro.rs:67). A Convocacao o copia cru (:337-347) e o bilhete o escreve cru (:409, e Display em seele-proto/src/uri.rs:315-318, :375), então o link sai com `enc=encontro.seele.app.br/<aviso>`. main.rs:1012 passa o campo `bilhete_guardado.ponto` cru para `onde_mora_hoje`, que passa a `onde_mora`, que faz `tokio::net::lookup_host(ponto)` (seele-core/src/encontro.rs:355). O tokio delega ao std (tokio-1.53.1 src/net/addr.rs, impl para str). MEDIDO com um programa rustc no scratchpad: `"encontro.seele.app.br".to_socket_addrs()` → `Err invalid socket address`; com `:8384` → `Ok [216.128.168.216:8384]`. Então `onde_mora` devolve None na hora, para as duas marcas. Contraste no mesmo arquivo: o `resolver` da Batida (seele-core/src/encontro.rs:228-231) usa `bilhete.ponto()`, que aplica a porta padrão, e por isso o LEVE funciona e o QUEM não. O mesmo código já estava em ce4875b (`git show ce4875b:...` mostra as três linhas idênticas). **Correção:** Estão construídos e publicados a fp estável, o histórico e o quarto no servidor. A consulta ao quarto pelo convidado existe no código, mas nunca sai para o ponto padrão: `fresco_do_server` e `fresco_do_aviso` são sempre None, e sem log nenhum (`.ok()?` engole o erro, e main.rs:1006-1030 não registra nada). O conserto continua pequeno, mas não é local ao anfitrião. É preciso normalizar a porta no convidado (usar `Bilhete::ponto()` em `onde_mora_hoje`) e/ou fazer o anfitrião escrever `:8384` no `enc=`, além do MORO com fp16.
- **REFUTADA**: 4. Basta atualizar o anfitrião: os clientes 0.15.0 já perguntam pela marca certa, então o conserto não sobe protocolo.
  - LIDO+MEDIDO, como na afirmação 3: o cliente 0.15.0 (e2fac4d) pergunta pela marca certa, mas o QUEM nunca sai do socket quando o `enc=` traz o ponto sem porta, que é o caso do ponto padrão (alcance/encontro.rs:67 → uri.rs:375 → main.rs:1012 → seele-core/src/encontro.rs:355). A marca certa só vale num `enc=` com porta explícita, e isso só acontece com `SEELE_ENCONTRO=host:porta` (encontro.rs:337-347). Nenhum pacote define essa variável: o grep em .rs/.json/.toml/.yml/.nsh/.sh não acha atribuição nenhuma. **Correção:** Um conserto só no anfitrião alcança clientes 0.15.0 apenas se o anfitrião passar a escrever a porta no bilhete (`encontro.seele.app.br:8384/...`), e só para links gerados depois do conserto. As entradas da trilha voltam a servir quando o convidado entra uma vez por um link novo, porque `anotar_caminhos` regrava o bilhete (main.rs:1264-1269). Links antigos e entradas antigas continuam mortos em 0.15.0. Para que um link já distribuído volte a funcionar, é obrigatório corrigir o cliente (`onde_mora_hoje` com `Bilhete::ponto()`). É mudança de comportamento, não de protocolo.
- **PARCIAL**: 5. Hoje a vida do servidor é a da janela e da sessão do anfitrião: SAIR, trocar de servidor e fechar a janela derrubam o servidor (resumo: «sem perguntar»), e não há bandeja, autostart, anti-sono nem seeled ligado.
  - Confirmado, LIDO: `disconnect` derruba a hospedagem (apps/seele-app/src/main.rs:1995-2020). Fechar a janela só chama `despedir_se`, que desconecta o cliente (main.rs:8110-8150), sem pergunta nenhuma. Não há tray, autostart, IOPM, SetThreadExecutionState nem inhibit (grep vazio em apps/seele-app/src, Cargo.toml, tauri*.json). O seeled vem só como externalBin (tauri.release.conf.json:4-6) e nenhum `Command::new` o lança. O seeled não sobe a escada: grep de Escada/encontro/Convocacao em crates/seele-server/src/main.rs vazio. Refutado, LIDO: SAIR pergunta quando se hospeda (ui/tela-chamada.js:776-784, `abrirConfirmacao`, com o texto «ele cai junto, e todo mundo que estiver nele sai» em ui/tela-sessao.js:3508-3511), e trocar de servidor também pergunta (tela-sessao.js:3529-3552). RECONECTAR preserva a hospedagem (`desmontar_a_sessao`, main.rs:~1985; ui/tela-fim.js:171-172). **Correção:** SAIR e trocar de servidor derrubam, mas perguntam antes e dizem o preço. Só fechar a janela derruba sem perguntar, e também sem `Hospedagem::encerrar`: a regra UPnP fica no roteador até VALIDADE=3600 s (alcance/porta.rs:86), e, pelo que se infere, quem estava dentro espera o tempo limite de ociosidade em vez de ser avisado. O resto está correto.
- **CONFIRMADA**: 6. Hospedar em outra versão (versões lado a lado) cria outra identidade de servidor, e os links dados antes são recusados por fp divergente.
  - LIDO: apps/seele-app/src/versoes.rs:218-223 (`dados: deposito.pasta_de_dados(&versao)`), com crates/seele-lancador/src/deposito.rs:170-172 (`raiz/dados/<versao>`). O teste em versoes.rs:432-460 cobra justamente `SEELE_HOME` = pasta da versão. `config_dir` lê `SEELE_HOME` primeiro (main.rs:848-851). A identidade mora no banco (tls.rs:66-88). `plano_de_dados`/`aplicar` (seele-lancador/src/dados.rs:127, :218) não têm chamador fora de dados.rs (grep). **Correção:** Vale só para o caminho de versões lado a lado. A atualização normal (updater no lugar) mantém `~/.config/seele` e, com ele, a identidade.

### Severidades contestadas

- **O quarto nunca devolve a escuta de avisos (marca «anfitriao» × impressão digital)**: o revisor propõe *bloqueante_1_0 (mantida), com escopo e custo corrigidos*. O defeito é real, mas não é o único elo quebrado nem o primeiro. O convidado nunca manda o QUEM ao ponto padrão (bilhete sem porta → `lookup_host` falha; medido). Então nem o endereço novo do servidor chega. O conserto não é «só no anfitrião, funciona com todo cliente 0.15.0». São três partes: (1) `onde_mora_hoje` passa a usar `Bilhete::ponto()` (cliente); (2) o anfitrião escreve `:8384` no `enc=`, para que clientes 0.15.0 com link novo funcionem; (3) o MORO da escuta de avisos com fp16, o filtro do eco e o primeiro MORO na subida, para as duas marcas. O guarda (d) como o analista o descreveu não pegaria o defeito da porta, porque todos os testes de quarto passam `127.0.0.1:porta` (seele-conformance/tests/quarto.rs:66-70). O ponto do teste tem de ser escrito exatamente como o link o carrega, montado por `Encontro::bilhete()`. Continua bloqueante porque «a URL não mudar» é pedido explícito do dono para a 1.0, e hoje a trilha e os links antigos falham, em silêncio para o convidado, em todo anfitrião atrás de NAT.
- **Frente B: «não derrubar sem perguntar» como mínimo da 1.0**: o revisor propõe *importante*. SAIR e trocar de servidor já perguntam e dizem o preço (tela-chamada.js:776-784; tela-sessao.js:3508-3552). Falta só confirmar ao fechar a janela enquanto se hospeda, e chamar `Hospedagem::encerrar` no ExitRequested (main.rs:8110-8125), para devolver o UPnP e avisar quem está dentro. É uma tarde de trabalho, não um bloqueio.
- **Impedir o sono enquanto hospeda**: o revisor propõe *importante*. Sem isto, o link persistente vale só enquanto o notebook do anfitrião não dorme, e o produto não diz nada. Não é bloqueante: servidor que dorme é recuperável ao acordar, desde que o quarto funcione. Mas é a primeira coisa que um usuário de «servidor sempre no ar» vai encontrar.
- **Identidade/dados entre versões lado a lado (afirmação 6)**: o revisor propõe *desejavel*. O caminho comum (updater no lugar) preserva `config_dir` e a chave. Só quem abre outra versão pelo launcher para hospedar perde a fp. Isso é nicho numa 1.0 em que todos estarão na mesma versão.

### O que o analista não viu

- O mais grave desta frente, e fora da análise: o QUEM do convidado nunca sai do socket para o ponto de encontro padrão. `PONTO_PADRAO = "encontro.seele.app.br"` não tem porta (crates/seele-server/src/alcance/encontro.rs:67). O `enc=` sai sem porta (encontro.rs:409; seele-proto/src/uri.rs:315-318, :375). `connect` passa o campo cru (apps/seele-app/src/main.rs:1012). `onde_mora` faz `tokio::net::lookup_host(ponto)` (crates/seele-core/src/encontro.rs:355), e o std recusa nome sem porta (MEDIDO: `Err invalid socket address`). Resultado: `onde_mora_hoje` devolve (None, None) sempre, desde ce4875b (conferido com `git show ce4875b:`). Nenhum log: `.ok()?` engole o erro, e main.rs:1006-1030 não registra o None. É o caso exato de «o produto sabe e não conta». A mesma função de resolução da Batida, ao lado, usa `bilhete.ponto()` com a porta padrão (seele-core/src/encontro.rs:228-231), e por isso o LEVE funciona e mascara o problema.
- O ADR 0047 §2.1 dá como «construído, e melhor que o proposto» o bilhete guardado ser substituído pelo endereço de hoje, e diz «isto foi medido, não deduzido» com `cargo test --test quarto`. Todos esses testes usam `127.0.0.1:<porta>` e MORO feito à mão (seele-conformance/tests/quarto.rs:47-70). A sondagem do coordenador também passa `encontro.seele.app.br:8384` com porta. Nenhuma medida até hoje exercitou o caminho do cliente como o link o carrega. Um guarda que existe e não prova o que diz provar.
- A URL identifica a máquina, não o servidor. `uma_chave_por_maquina` (apps/seele-app/src/servidores.rs:233-297) planta a mesma chave em todo servidor guardado, então a fp e as marcas do quarto são iguais para todos. A porta é fixa (8383), e o link não tem id de servidor: as chaves lidas são alt/enc/room/v/fp/convite (seele-proto/src/uri.rs:440-480). O link antigo do servidor A abre, sem aviso nenhum, o servidor B que a máquina hospedar hoje. O membro que volta cai na portaria de B como desconhecido. Com multiconexão (ADR 0031), dois servidores da mesma máquina disputariam a mesma marca no quarto (primeiro que escreve fica, seele-encontro/src/lib.rs:113-139). Importante para a 1.0 se o registro de vários servidores (ecf0f0b) for vendido como «servidor persistente».
- O primeiro registro no quarto das duas marcas (inclusive a fp16+s do socket do servidor, não só a do aviso) só sai 15 s depois de hospedar: `relogio.tick().await` consome o tique imediato (alcance/encontro.rs:790-791, REAVIVAR=15 s em :145). Quem volta nesses 15 s recebe None e cai no endereço de ontem.
- O quarto recusa em silêncio a mudança de IP público por até 60 s: `morar` só aceita outro endereço se o IP for o mesmo ou se o prazo tiver vencido (seele-encontro/src/lib.rs:125-133, PRAZO_DO_QUARTO=60 s). Um anfitrião que reinicia logo depois de trocar de rede, ou de receber outro IP do CGNAT, fica até um minuto apontado para o endereço velho. Desejável.
- `onde_mora` usa o primeiro registro do DNS (`.next()`, seele-core/src/encontro.rs:355). É o mesmo padrão que o arquivo documenta como defeito de campo e já corrigiu na Batida (`escolher_ponto`, :238-262). Numa máquina com AAAA primeiro e IPv6 sem rota, o QUEM some e o prazo de 500 ms (seele-ffi/src/lib.rs, PRAZO_DO_QUARTO) vence em silêncio. O quarto é compartilhado entre famílias (seele-encontro/src/main.rs:62-66), então basta tentar os dois endereços em sequência ou preferir IPv4. Importante, depois do conserto da porta.
- Fechar a janela enquanto hospeda não chama `Hospedagem::encerrar` (main.rs:8110-8125 só chama `despedir_se`, que desconecta o cliente). A regra UPnP de 8383 fica no roteador por até VALIDADE=3600 s (alcance/porta.rs:86). O comentário de `despedir_se` descreve, para o cliente, o problema de quem fica na sala até o tempo limite de ociosidade. O servidor hospedado que morre com o processo não recebeu o mesmo tratamento (INFERIDO: o Drop de `Hospedagem` não roda no `exit` do Tauri).
- O link «persistente» ainda carrega convite de uso único (hospedagem.rs:315-328; main.rs:7137-7165), e a portaria vem ligada por padrão (`semear_ligada`, main.rs:~1580). Um link fixo publicado num grupo leva o primeiro a entrar direto e todos os outros à fila de aprovação. Não é defeito, mas define o que «URL que não muda» entrega na 1.0 e precisa estar dito na interface.

### Desenho proposto pelo analista

## O desenho: o link é a impressão digital; os endereços são palpites; o quarto responde onde ela mora hoje

Isso já é a arquitetura do código. Falta fechar um elo e parar de derrubar o servidor sem perguntar. Nenhuma etapa abaixo sobe o protocolo `SEELE-ENC/1` nem o protocolo QUIC.

### Etapa 1 — Consertar o quarto (bloqueante; horas de código e um dia de campo)
Só o anfitrião muda, e todo cliente 0.15.0 passa a reencontrar. Dá para sair como 0.15.1.
1. `alcance/encontro.rs:816`: `moro(&de_quem_chega)` no lugar de `moro(&minha_marca)`.
2. Em `atender`, mandar também um `ONDE anfitriao` no tique e guardar o reflexo da escuta de avisos. Um `AQUI <fp16>` cujo endereço é esse reflexo é eco do próprio MORO e não gera furo. Isso também detecta remapeamento: reflexo ≠ `aviso` vai para o log agora e para a tela na etapa 3.
3. Primeiro MORO na subida: tirar o `tick()` consumido em `:791`, ou mandar antes do laço.
4. Guarda de ponta a ponta em `crates/seele-conformance/tests/quarto.rs`: `seele_encontro::Ponto::abrir_com_quarto` com `Vizinhanca::TambemAqui`, um socket de servidor em 127.0.0.1, `seele_server::alcance::encontro::abrir(&Convocacao{…})` (módulos já públicos) e `seele_ffi::onde_mora_hoje`. Afirma os dois `Some`, o aviso igual a `bilhete().aviso`, e nenhum furo contra o próprio reflexo. **Provar revertendo:** voltar para `minha_marca` tem de deixar o teste vermelho.
5. Rodar esse teste à mão enquanto o CI estiver só em `workflow_dispatch`, e escrever isso nas notas.
6. Campo: host em CGNAT ou sem UPnP, convidado no 5G. O host fecha e abre; o convidado (a) volta pela trilha e (b) cola o link de antes. Só depois disso trocar a frase `FuroDeNat` (frases.js:770).

### Etapa 2 — Nenhuma falha calada no caminho do link (P a M; depende da 1 só para a frase)
- `onde_mora_hoje` faz um `ONDE` em paralelo. Se o ponto respondeu e o quarto calou, `ConnectionError::AnfitriaoForaDoAr` com frase própria.
- Na tela de hospedar, por degrau: «este link continua valendo depois de fechar?» (sim pelo quarto; sim enquanto o IP não mudar; não, use um nome).
- Nome público guardado em `servidores.json` e reaplicado ao hospedar.
- Cortar `caminhos` em 4 e não gravar o endereço que veio do quarto.
- Política de convites (decisão do dono, lacuna 4). A implementação é pequena em `admissao.rs:94-107` ou `session.rs:664-668`.

### Etapa 3 — O servidor sobrevive ao que a pessoa faz na janela (M; independente da 1)
Reusa `Session.hospedagem`, que já é independente de `Session.connection`, e a portaria, que já é administrada pelo banco.
1. **SAIR e trocar de servidor perguntam**: «sair e manter o servidor no ar» ou «encerrar». Manter é não fazer `take()` em `disconnect` (main.rs:2014-2020). Com isso o host visita um amigo enquanto hospeda, sem multiconexão: uma conexão de cliente e um servidor local.
2. **Fechar a janela com gente dentro pergunta.** Ao sair de verdade, `encerrar()`, que devolve o UPnP.
3. **Não dormir enquanto hospeda com alguém dentro**: `IOPMAssertionCreateWithName` no macOS, `SetThreadExecutionState` no Windows, `systemd-inhibit` no Linux. Se não der, a tela avisa.
4. Mostrar o remapeamento detectado na etapa 1.2.

### Etapa 4 — «Sempre no ar» (G; candidata à 1.1, a menos que o dono a queira na 1.0)
- Bandeja: a janela esconde e o servidor segue. Iniciar com o sistema usando `--hospedar --servidor <id>`, reaproveitando `Abertura` (main.rs:278-292).
- `seeled --servidor <id>` resolve o banco pela mesma regra de `config_dir` (mesmo banco, mesma chave, mesmo link); `seeled --casa` sobe pela `Hospedagem::iniciar` (escada + quarto) em vez de `Daemon::bind`; semeia a portaria como o app faz; trava o banco contra dois processos. Mais uma unidade systemd e um LaunchAgent de dez linhas cada, no modelo de docs/ponto-de-encontro.md:106.

### Etapa 5 — Versões (M; decisão antes de código)
- Regra para a 1.x: negociação que aceita uma faixa de protocolo dentro da versão maior. É a única forma de «o link continua funcionando» sobreviver a uma atualização.
- Ao hospedar em outra versão, herdar `identidade-servidor.db` e `identity.key` para a pasta de dados dela, ligando `seele-lancador::dados`, que já existe. Ou dizer na tela que é outro servidor, com outro link.
- `Conhecido` ganha a versão para a trilha poder abrir a versão do servidor.
- Histórico chaveado pela impressão digital quando ela existir (lacuna 10).

**Ordem e dependências:** 1 → frase de 2 → 3 (em paralelo com 2) → 4 e 5. Para a 1.0: etapa 1 inteira, etapa 2 menos a política de convites se o dono não decidir, e os itens 1 a 3 da etapa 3. Fica fora da 1.0: registro assinado no quarto (recusado no ADR 0047 §2.4, e nada mudou) e diretório de nomes próprio.

### Perguntas ao dono

- Um link aponta para a máquina ou para um servidor guardado? Hoje todos os servidores da máquina compartilham a chave (servidores.rs:233-297), e o link da «Mesa de RPG» leva ao servidor que estiver no ar.
- Depois de emitir um convite de uso único, o link simples de HOSPEDAR deve continuar pondo gente nova na fila da portaria, ou deixar de admitir? Hoje deixa, sem avisar ninguém (admissao.rs:94-107).
- A 1.0 promete compatibilidade de protocolo dentro da 1.x? Sem isso, cada atualização quebra todos os links e a trilha entre quem atualizou e quem não atualizou.
- «Servidor sempre no ar» (bandeja, iniciar com o sistema, seeled como serviço com o mesmo banco do app) entra na 1.0 ou fica para a 1.1?
- Pode o anfitrião SAIR, ou visitar o servidor de um amigo, mantendo o próprio servidor no ar (hospedar sem estar dentro)? É o recorte barato que evita a multiconexão.
- A promessa «o link continua valendo depois de fechar» passa a depender do ponto de encontro do projeto (o quarto em memória esvazia por até 15 s num reinício). Aceita um ponto só, ou quer um segundo, para redundância, antes de escrever essa promessa na tela?
- Hospedar em outra versão deve herdar a identidade do servidor e a da pessoa (mesmo link, mesma conta), ou ser explicitamente outro servidor?

---

## URL amigável

**Resumo do analista.** No HEAD, que é a v0.15.0 publicada, o convite é um `seele://` de 100 a 255 caracteres. Ele é validado com cuidado e evolui bem por parâmetros opcionais (uri.rs), mas **não abre o app com um clique**. O esquema não está registrado em nenhum sistema operacional, não há instância única e o app não trata URL recebida do SO. Portanto, em WhatsApp, Discord ou Telegram o link é texto para copiar e colar. Esse é o maior ganho de "amigável" e custa M. O "link com nome" (c42a30f, publicado desde a v0.11.0) funciona no caso feliz, mas tem 6 furos silenciosos: GERAR CONVITE descarta o nome, o nome não fica salvo por servidor, só o primeiro endereço DNS do nome é usado, IPv6 cru passa na conferência, o `seeled` não tem nome nenhum e nada avisa que nome com CGNAT não funciona. Mais grave, e pré-requisito de qualquer resolução por nome ou código: a `fp` do link só é conferida **depois** do aperto de mão, que já entregou o token de uso único e uma assinatura da identidade. Além disso, quem volta pela lista para um endereço dado pelo quarto (QUEM) entra com "primeiro contato" cego, embora a `fp` esteja guardada. Recomendação para a 1.0, em quatro partes: (0) conferir a `fp` dentro do TLS e usar a `fp` guardada; (1) consertar o link com nome; (2) tornar `seele://` clicável, com confirmação antes de conectar; (3) oferecer a página estática `https://seele.app.br/e#<corpo>`, que só repassa o fragmento para `seele://`. Código curto resolvido pelo ponto de encontro e nomes registrados nele ficam fora da 1.0: dão ao operador poder de se passar pelo servidor, ou exigem estado em disco. A variante autocertificante, derivada da `fp`, é o caminho da 1.x.

### O que existe

- `construido_e_publicado`: Gramática seele:// com alt/enc/fp/convite/room/v e regra de parâmetro desconhecido ignorado (crates/seele-proto/src/uri.rs:67-490)
- `construido_e_publicado`: Link gerado pela hospedagem do app com todos os candidatos da escada, fp e v (crates/seele-server/src/hospedagem.rs:199-231)
- `parcial`: Link com nome (campo opcional na hospedagem, conferência sintática, nome vira alvo) (uri.rs:1171-1196; hospedagem.rs:262-290; main.rs:1520-1527,1613; index.html:369-395 (publicado desde v0.11.0))
- `construido_e_publicado`: Receita de DNS/DDNS para quem hospeda (docs/alcance-pela-internet.md:53-112)
- `parcial`: Conferência da fp do link contra o certificado (crates/seele-core/src/enlace.rs:1263-1291,4607-4627; tofu.rs:113-146)
- `construido_e_publicado`: Abrir o link na versão que o servidor roda (v= + launcher, --entrar) (apps/seele-app/src/main.rs:255-294,1451-1497,6918-6950; apps/seele-app/ui/tela-boot.js:567-596)
- `construido_mas_nao_ligado`: Argumento --entrar <link> pronto para receber um link vindo do SO (apps/seele-app/src/main.rs:278-294 (só o launcher o usa, versoes.rs:215))
- `ausente`: Registro do esquema seele:// no macOS/Windows/Linux (ausente em tauri.conf.json:56-70, Info.plist, apps/seele-instalador/src/registro.rs; janela.rs:271-275 admite)
- `construido_mas_nao_ligado`: Bloco deep_link_protocols no template NSIS (apps/seele-app/instalador.nsi:746-750 (NSIS não é mais publicado, ADR 0043))
- `ausente`: Instância única / entrega de URL ao app já aberto (nenhum plugin em apps/seele-app/Cargo.toml; main.rs:8108-8125 sem RunEvent::Opened)
- `ausente`: Página https de redirecionamento para seele:// (seele.app.br está no ar via Cloudflare (medido), sem rota de convite)
- `construido_e_publicado`: Quarto MORO/QUEM (endereço de hoje pela marca da fp) (crates/seele-encontro/src/lib.rs:68-151; seele-ffi/src/lib.rs:7797-7818; main.rs:1001-1030)
- `so_desenho`: Código curto / nome legível resolvido pelo ponto de encontro (docs/adr/0047-o-link-que-volta-a-funcionar-amanha.md §2.4-2.5 (recusado/deixado como pergunta))
- `ausente`: seele:// clicável dentro do chat do próprio app (apps/seele-app/ui/tela-sessao.js (nenhum tratamento))

### Lacunas

- **[importante · esforço M · risco medio] seele:// não é clicável: esquema não registrado, sem instância única e sem tratamento de URL recebida do SO.** É o maior ganho de 'amigável'. Quem recebe o convite no WhatsApp, Discord ou Telegram precisa copiar o texto, abrir o app, abrir a lista de servidores e colar. Faltam quatro coisas: o registro do esquema (CFBundleURLTypes no macOS, HKLM\Software\Classes\seele no instalador próprio do Windows, MimeType=x-scheme-handler/seele no .desktop do .deb); a entrega da URL (RunEvent::Opened no macOS, argv solto no Windows/Linux); a instância única, que repassa a URL à janela aberta; e a tela de confirmação antes de conectar, exigida pelo ADR 0006 alt. 3. Também falta decidir o que fazer quando já há sessão ou hospedagem (AlreadyConnected).
  - *Evidência:* LIDO: tauri.conf.json:56-70 (só updater); Info.plist sem CFBundleURLTypes; main.rs:278-294 (argv ignora seele:// solto); main.rs:8108-8125 (sem Opened); main.rs:899 AlreadyConnected; apps/seele-instalador/src/janela.rs:271-275; registro.rs:34-40; docs/pendencias.md:427-431. MEDIDO: latest.json v0.15.0 publica SEELE_0.15.0_x64-instalador.exe (o NSIS com deep_link_protocols não sai).
- **[importante · esforço P · risco medio] A fp do link é conferida depois do aperto de mão, que já entregou o token e uma assinatura da identidade.** O TofuVerifier aceita qualquer certificado no primeiro contato e não conhece a fp esperada. O handshake manda Hello{join_secret} e assina o nonce do servidor. Só depois conferir() recusa. Qualquer um em posição de atender um candidato recebe o token de uso único e uma assinatura válida sobre um nonce que ele escolhe (INFERIDO, não executado): quem tem o IP da LAN do link na rede do convidado (discado em t=0), um Wi-Fi malicioso, o DNS do nome, a marca do quarto. Como a assinatura não é vinculada ao canal TLS, dá para repassá-la ao servidor real e entrar como o convidado, gastando o convite dele. Hoje o risco é baixo em volume. Mas passa a ser bloqueante no dia em que a 1.0 resolver nome ou código por terceiro: é a fp conferida antes de tudo que permite dizer que um resolvedor 'pode mentir sobre onde, mas não sobre quem'. O conserto mínimo é passar a fp esperada ao TofuVerifier e falhar o TLS antes do Hello. O vínculo com o canal (assinar nonce || exporter TLS) é mudança de protocolo, para a frente de segurança.
  - *Evidência:* LIDO: tofu.rs:227 (new sem expected), :246-257, :276-284; client.rs:560-575, :1656-1705 (Hello com join_secret, depois Response=sign(nonce)); enlace.rs:1263-1291 (conferir só depois de connect_por); session.rs:559-600 (verifica assinatura sobre nonce cru); grep por export_keying_material vazio; seele-conformance/tests/convite.rs:19-23 admite a recusa pós-aperto e testa com segredo: None.
- **[importante · esforço P · risco baixo] Quem volta pela lista entra com 'primeiro contato' cego no endereço dado pelo quarto, com a fp guardada sem uso.** No retorno pela lista, expected_fingerprint é None: `esperada` só vem de link colado nesta sessão. A impressao_guardada da lista serve só para a marca do QUEM. O endereço fresco do quarto entra como alt[0] com chave de pino nova, portanto FirstContact cego. Toda vez que o mapeamento de NAT muda, quem volta lê 'Ninguém confirmou que é esta', o que ensina a clicar sem ler. Um impostor que ocupa a marca enquanto o anfitrião está fora (quem escreveu primeiro fica) captura esses retornos. O ADR 0022 e seele-encontro dizem o contrário. O conserto é uma linha: `expected_fingerprint: esperada.or(impressao_guardada)`, mais um teste de conformidade com servidor que troca de porta.
  - *Evidência:* LIDO: apps/seele-app/src/main.rs:928-948 (esperada só do slot do convite), :960-983 (impressao_guardada da lista), :1001-1030 (usada só no onde_mora_hoje), :1079 (expected_fingerprint: esperada); seele-ffi/src/lib.rs:3749-3760 (pino por texto do endereço); tela-sessao.js:235-240; seele-encontro/src/lib.rs:115-140, :429-433; docs/adr/0022...md:541-547.
- **[importante · esforço P · risco baixo] GERAR CONVITE (token) descarta o nome público do link.** Com nome público, o link inicial sai com o nome. Com a portaria ligada por padrão, o caminho normal para convidar alguém é gerar um convite de uso único. Esse link é montado a partir do convite numérico e sobrescreve o link com nome nos dois campos da porta, sem aviso. É o 'produto sabe e não conta': quem hospeda pediu nome e recebe um link que muda amanhã.
  - *Evidência:* LIDO: crates/seele-server/src/hospedagem.rs:322-329 (convite_com_token usa self.convite()); apps/seele-app/src/main.rs:7142-7167 (não recebe o nome); ui/camada-portaria.js:611-619 e :659-665 (guardarOLinkDaPorta sobrescreve os dois campos).
- **[importante · esforço P · risco baixo] O nome público não é salvo por servidor: reabrir o app e hospedar o mesmo servidor volta ao link numérico.** O nome mora num input dentro de um <details> fechado e não persiste. `Servidor` (servidores.json) não tem campo para ele. `hospedarGuardado` sobe o servidor salvo 'sem passar por tela nenhuma', lendo o campo vazio. O link antigo com nome continua valendo, mas todo link novo sai numérico e nada avisa. Como o nome existe justamente para o link durar, perdê-lo em silêncio anula a funcionalidade.
  - *Evidência:* LIDO: apps/seele-app/src/servidores.rs:44-62; ui/index.html:369-395; ui/tela-boot.js:428-430, :695-701 (hospedarGuardado).
- **[importante · esforço P · risco baixo] O nome resolve para um endereço só (o primeiro do getaddrinfo).** resolve() pega `.next()` de to_socket_addrs. A doc manda criar A e AAAA. Se o primeiro endereço for o IPv6 e o firewall do roteador barrar entrada (padrão doméstico), o candidato do nome falha mesmo com o A funcionando (INFERIDO). Os numéricos no `alt` só salvam enquanto o IP não mudou, e não mudar é o motivo de ter nome. Todos os endereços resolvidos deveriam virar candidatos na corrida do ADR 0037, respeitando o limite.
  - *Evidência:* LIDO: crates/seele-ffi/src/lib.rs:3705-3711; docs/alcance-pela-internet.md:65-67; docs/adr/0037-candidatos-do-convite-em-paralelo.md (corrida com defasagem de 250 ms).
- **[importante · esforço P · risco baixo] Nome público sem coerência com o alcance: porta externa, CGNAT e 'o nome aponta para outro IP'.** O nome sai com a porta 8383 implícita mesmo quando o UPnP abriu outra (porta_nao_canonica, que o app já conhece). Com degrau 4 (CGNAT, comum em conexões residenciais brasileiras) ou SoRedeLocal, o nome aponta para um IP que ninguém alcança, e nada é dito. O app conhece o próprio IP público (ONDE/escada) e poderia resolver o nome e avisar 'o nome aponta para X, sua casa está em Y'. Hoje a conferência é só sintática, por decisão escrita.
  - *Evidência:* LIDO: crates/seele-server/src/alcance.rs:952-962 (porta_nao_canonica); hospedagem.rs:262-290 (nome sem porta); uri.rs:1161-1166 (não confere DNS, de propósito); main.rs:1613-1620 (alcance e degrau já disponíveis no Anfitriao).
- **[importante · esforço M · risco baixo] Não existe a forma https do convite: página que repassa para seele:// e app que aceita o link colado.** Para o convite ser clicável em qualquer conversa, o link que se manda precisa ser https. Falta a página estática `https://seele.app.br/e#<corpo>` e falta o `uri::analisar` aceitar esse prefixo. Hoje, colar um https no campo de servidores vira 'endereço cru' e falha com UnresolvableHost: camada-servidores.js:169 só reconhece `seele://`. O domínio e a hospedagem estática já existem (medido). Sem o registro do esquema (lacuna 1), a página só consegue oferecer 'copiar' e 'baixar'.
  - *Evidência:* LIDO: uri.rs:412-414 (só seele://); apps/seele-app/ui/camada-servidores.js:169; tela-boot.js:581-582. MEDIDO: curl https://seele.app.br/ → 200, server: cloudflare.
- **[importante · esforço P · risco baixo] `seeled convite` não aceita nome, nem alt, enc ou v.** Quem hospeda em VPS com domínio, que é o caso típico do daemon, recebe um link com o IP da LAN da VPS (ou SEU-ENDERECO:8383) e precisa editá-lo à mão. Falta `seeled convite --nome casa.exemplo.br` (ou SEELE_NOME_PUBLICO), passando por conferir_nome_publico e analisar.
  - *Evidência:* LIDO: crates/seele-server/src/main.rs:180-218 e :150-163 (uso); grep por nome_publico em seele-server/src/main.rs vazio.
- **[desejável · esforço P · risco baixo] conferir_nome_publico aprova textos que o analisador do convidado recusa.** A conferência aceita qualquer combinação de alfanumérico, ponto, hífen e dois-pontos. `2001:db8::1`, `casa:` e `casa:abc` passam e geram um link que o outro lado recusa com EnderecoIpv6SemColchetes ou EnderecoInvalido. A função existe para impedir exatamente isso, e o teste de ida e volta usa um nome bom só. Conserto: aprovar só o que `analisar(&format!("seele://{nome}"))` aceita, e testar os três casos tortos.
  - *Evidência:* LIDO: crates/seele-proto/src/uri.rs:1189-1194 vs :582-595 e :553-557; teste único em :1263-1272.
- **[desejável · esforço P · risco baixo] convite_com_nome pula o LIMITE_DE_ALVOS e o 5º candidato some calado.** O struct é montado à mão com alvo antigo inserido em alt[0]. Com 4 numéricos, sai um link de 5 alvos e o leitor descarta o último (Global ou Túnel) sem dizer nada. Deveria passar por com_alternativos.
  - *Evidência:* LIDO: crates/seele-server/src/hospedagem.rs:274-286; uri.rs:161-176 e :450-452; alcance.rs:575.
- **[desejável · esforço P · risco baixo] Documentação que descreve caminhos inexistentes ou superados.** README:220 manda usar `connection --url`, e esse binário não existe mais. O comentário de `Abertura` descreve o clique num seele:// que não é possível. O cabeçalho de chegada.rs diz que os candidatos são tentados em série (ADR 0037 trocou por corrida). A nota do TOFU na tela do nome ('vai ser perguntado de novo') vale para quem volta sem fp, não para quem chega por link com fp, que vê 'o convite já confirmou'.
  - *Evidência:* LIDO: README.md:220; Cargo.toml dos binários (só seeled, seele-encontro, seele-app); main.rs:262-267; crates/seele-core/src/chegada.rs:12-16; index.html:389-393; tofu.rs:113-127.
- **[desejável · esforço M · risco medio] Quarto: MORO do dono legítimo recusado em silêncio quando um impostor ocupou a marca.** 'Quem escreveu primeiro fica' sem autenticação. Um impostor que registra a marca, derivada de 16 hex da fp que está em todo link, enquanto o anfitrião está fora, mantém a vaga reavivando. O anfitrião volta, o MORO dele é ignorado, e ele recebe a mesma resposta de sempre: a persistência do link morre sem ninguém saber. Não é da 1.0 se o link não depender do quarto. É pré-requisito se um link curto ou estável passar a depender dele (MORO assinado pela chave do servidor).
  - *Evidência:* LIDO: crates/seele-encontro/src/lib.rs:115-140 (morar), :273-275 (MORO respondido como ONDE); seele-ffi/src/lib.rs:7797-7818 (marca = fp[..16]); docs/adr/0047...md:143-170 (camada assinada recusada).

### O que o revisor conferiu

- **CONFIRMADA**: 1. O esquema seele:// não está registrado em nenhum sistema e o app não trata URL entregue pelo SO; hoje clicar num convite não abre o SEELE.
  - LIDO: apps/seele-app/tauri.conf.json:57-70 tem só o plugin `updater`. apps/seele-app/Info.plist tem só NSLocalNetwork/NSScreenCapture/NSMicrophoneUsageDescription. apps/seele-app/Cargo.toml:36-58 tem só tauri, tauri-plugin-dialog e tauri-plugin-updater. Um grep por `CFBundleURL|deep.?link|single.?instance|MimeType|x-scheme-handler|RunEvent::Opened` em apps/ e crates/ acha apenas o template apps/seele-app/instalador.nsi:746-750 e :881. main.rs:8111-8125: o handler de `app.run` só trata `ExitRequested`. main.rs:283-292: `Abertura::da_linha_de_comando` lê apenas --hospedar, --nome-publico e --entrar; um `seele://` posicional (a forma em que Windows e Linux entregam a URL, `%1`) cai em `_ => {}`. MEDIDO: latest.json da v0.15.0 publica darwin-aarch64 (.app.tar.gz) e windows-x86_64 `SEELE_0.15.0_x64-instalador.exe`, que é o seele-instalador e não o NSIS. Grep por Classes/protocol em apps/seele-instalador/src não acha nada, e janela.rs:271-275 admite a ausência. MEDIDO nesta máquina: /Applications/SEELE.app é a 0.14.2, e o plist dela não tem CFBundleURLTypes; `lsregister -dump | grep -ci 'seele:'` → 0. **Correção:** Acrescento um detalhe: o comentário em apps/seele-instalador/src/main.rs:225-227 («o seele-app não lê argv») está desatualizado, porque desde o ADR 0046 o app lê --entrar. Além disso, a reabertura via explorer.exe descarta qualquer argumento, então um link nunca sobrevive ao caminho do atualizador.
- **CONFIRMADA**: 2. A fp do convite é conferida depois do handshake, e esse handshake já entregou join_secret e uma assinatura do nonce do servidor.
  - LIDO: tofu.rs:264-287: `verify_server_cert` não recebe fp esperada e aceita `FirstContact`. client.rs:549-583: o TLS termina e `handshake` roda completo. client.rs:1664-1690: o `Hello` leva `join_secret` e `public_key`, e a `Response` é `signing_key.sign(&nonce)` sobre o nonce cru. enlace.rs:1263-1291: `conferir` só roda depois de `Client::connect_por` devolver. session.rs:559-600: o servidor verifica `verify(&nonce, …)` sem exporter TLS nem contexto. O próprio teste admite isso em crates/seele-conformance/tests/convite.rs:246-248: «A recusa vem depois do aperto de mão, com a sessão já de pé lá». **Correção:** A consequência mais grave não está na ordem da conferência. Está na assinatura sem vínculo com o canal. A identidade é uma só por máquina (seele-ffi/src/lib.rs:246-254, identity.key), e o dono de um servidor hospedado é admitido por essa mesma chave (main.rs, `admitir_o_dono`). Por isso qualquer servidor que a pessoa V visite (A) pode abrir uma conexão com um servidor B, mandar o Hello com a chave pública de V, repassar o nonce de B como se fosse o seu e reenviar a assinatura de V. A entra em B como V, inclusive como dono se V hospeda B (INFERIDO a partir do código LIDO). Conferir a fp dentro do TLS, o item (0) do analista, não fecha esse ataque: V foi a A de propósito e a fp de A confere. O conserto é assinar material exportado do TLS (quinn/rustls `export_keying_material`) ou, no mínimo, rótulo+fp do servidor+nonce. Isso é mudança de protocolo. Outro ponto: `join_secret` também transporta a senha do servidor (admissao.rs:143-151). Um cliente que use senha a entrega a qualquer servidor antes de qualquer conferência. O app só manda o token (tela-boot.js:203).
- **CONFIRMADA**: 3. No retorno pela lista, a fp guardada não é usada como fp esperada, e o endereço vindo do quarto entra como primeiro contato cego.
  - LIDO: `esperada` sai só de `session.convite` (main.rs:928-948). Esse slot é escrito apenas por `analisar_convite` (main.rs:6945-6946) e apagado ao desconectar (main.rs:2089). O clique na lista manda só o `alvo` (camada-servidores.js:55-95). `impressao_guardada` (main.rs:966-983) é usada somente para perguntar ao quarto (main.rs:1006-1027). A resposta do quarto entra no índice 0 (main.rs:1011-1015), e `expected_fingerprint: esperada.clone()` (main.rs:1079) segue None. O pino é chaveado pelo texto `host:porta` (seele-ffi/src/lib.rs:3729 e :3748-3760), então uma porta nova vira `FirstContact`, e a tela mostra «PRIMEIRO CONTATO — CHAVE FIXADA … Ninguém confirmou» (tela-sessao.js:235-240). Não medi em execução. **Correção:** O efeito é pior do que o analista descreve. Sem `impressao_esperada`, `Chegada::nova` marca `com_bilhete_e_impressao=false` (chegada.rs:525-528), `ponto_a_avisar` devolve None (chegada.rs:664-668) e `Batida::preparar(bilhete, None)` devolve None (encontro.rs:93-99, chamada em enlace.rs:994-1001). Resultado: no retorno pela lista nenhum LEVE sai e o anfitrião nunca fura o NAT. O mesmo acontece na reconexão (enlace.rs:2707-2715). O quarto acha o endereço novo, mas para um anfitrião atrás de CGNAT sem UPnP a conexão não passa, e isso é silencioso.
- **CONFIRMADA**: 4. O fragmento (#…) de uma URL https não é enviado ao servidor que hospeda a página, então a página /e não vê o convite.
  - MEDIDO: abri https://seele.app.br/e#teste-fragmento-xyz no painel de navegador, e read_network_requests mostra `GET https://seele.app.br/e → 200`, sem o fragmento. Mesma medida, porém: a página hoje carrega o Cloudflare Web Analytics injetado (`static.cloudflareinsights.com/beacon.min.js` e `POST /cdn-cgi/rum`), a resposta não tem cabeçalho Content-Security-Policy, e o fallback de SPA devolve 200 para qualquer caminho (`/rota-que-nao-existe-*` → 200). Hoje /e serve a landing. O código atual do beacon limpa a URL (`e.hash="",e.search=""`), mas é código de terceiro rodando com acesso a `location.hash`. **Correção:** A tese vale. A pré-condição que o analista impõe («Cloudflare sem injeção», CSP sem terceiros) é hoje FALSA no site publicado. Desligar a injeção automática do Web Analytics, pelo menos em /e, e servir /e com CSP própria são passos obrigatórios. O código do site não está neste repositório (git ls-files não tem fonte do site), então esse trabalho acontece fora do monorepo.
- **NAO_VERIFICAVEL**: 5. WhatsApp, Discord e Telegram não tornam seele:// clicável, então sem a forma https o convite continua sendo copiar e colar.
  - Não medi, e o analista também não. Pelo conhecimento público é provável para WhatsApp e Discord, que só transformam em link http(s)/www; para o Telegram não tenho certeza. **Correção:** Mesmo que algum desses apps torne seele:// clicável, o registro no SO (afirmação 1) continua sendo pré-requisito. A página https também continua útil para quem ainda não tem o app, porque pode oferecer o download.
- **PARCIAL**: 6. Código curto aleatório ou nome resolvido pelo ponto de encontro dá ao operador o poder de indicar outra chave; só um código derivado da fp preserva a invariante do ADR 0022.
  - LIDO: o quarto não tem autenticação. seele-encontro/src/lib.rs:109-138 («não é autenticação»), e :284-293 mostra que MORO grava o endereço de origem de quem mandar e QUEM responde a qualquer um. ADR 0022:119-122 declara a invariante «não consegue … se passar por ninguém». **Correção:** (a) Essa invariante já está quebrada no HEAD pelo caminho da afirmação 3. A marca é `fp[..16]` (encontro.rs:93-99) e está em todo link com `fp=`. Quem já recebeu um link pode fazer MORO quando o anfitrião fica fora do ar por mais de 60 s (lib.rs:73). Reavivando a cada 15 s, ainda impede o anfitrião de voltar ao quarto, porque «quem escreveu primeiro fica» (lib.rs:118-125). Quem volta pela lista cai nele como FirstContact cego. A defesa documentada em seele-proto/src/encontro.rs:65-67 («quem chega confere a impressão digital de qualquer jeito») e :84-88 («quem recebe o convite nunca lê resposta nenhuma daqui») é falsa diante de main.rs:1011-1015 e :1079. (b) «Código curto autocertificante» é quase uma contradição. A fp é SHA-256 do DER do certificado (tofu.rs:247), e dá para moer serial e validade sem gerar chave nova. Um código de 6 a 8 caracteres base32 (30 a 40 bits) cai em minutos numa GPU; seriam precisos uns 16 caracteres (~80 bits) (INFERIDO).
- **PARCIAL**: (existe) Argumento --entrar pronto para receber um link vindo do SO.
  - LIDO: main.rs:288 lê `--entrar <link>`, e tela-boot.js:579-582 faz `analisar_convite` e em seguida `await conectar(...)` direto, sem nenhuma pergunta. **Correção:** Ele não está pronto para o SO. Ligar o handler do esquema a --entrar criaria um link que conecta sozinho, o que viola docs/pendencias.md:427-431 («precisa perguntar antes de conectar») e, somado à afirmação 2, entregaria token e assinatura com um clique. O argumento posicional também é ignorado (main.rs:291).
- **PARCIAL**: (existe) Quarto MORO/QUEM construído e publicado, dando o endereço de hoje.
  - LIDO: o serviço e a pergunta existem (lib.rs:68-151; main.rs:1006-1027). O teste crates/seele-conformance/tests/quarto.rs:42 cobre só `onde_mora` devolvendo o endereço, não a conexão que vem depois. **Correção:** Ele acha o endereço, mas a conexão que o usa sai sem fp esperada e sem furo de NAT (ver afirmação 3). É o caso de «existir não é funcionar»: nenhum teste exercita lista → quarto → conexão.
- **CONFIRMADA**: (furos do link com nome) GERAR CONVITE descarta o nome; nome não salvo; só o 1º endereço DNS; IPv6 cru passa; seeled sem nome.
  - LIDO: `criar_convite_do_server` (main.rs:7142-7166) chama `convite_com_token` (hospedagem.rs:322-329), que parte de `self.convite()` numérico. `Servidor` (servidores.rs:44-62) não tem campo de nome público, e `campo-nome-publico` só é lido em tela-boot.js:428, :547 e :977. `resolve` usa `.next()` (seele-ffi/src/lib.rs:3707-3711). `conferir_nome_publico` aceita `:` e dígitos (uri.rs:1187-1193). O `seeled convite` usa só o endereço de LAN ou o texto `SEU-ENDERECO:8383`, sem nome, sem alt e sem v (crates/seele-server/src/main.rs:180-216). **Correção:** O furo do IPv6 é pior do que «passa na conferência». `2001:db8::1`, `casa:abc` e `casa.exemplo:99999` passam, `convite_com_nome` (hospedagem.rs:262-290) não reanalisa o que montou, e do outro lado `analisar` → `validar_alvo` (uri.rs:582-595) → `separar` (uri.rs:553-557) recusa o LINK INTEIRO, alternativas numéricas incluídas. O guarda `o_que_passa_daqui_analisa_como_link` (uri.rs:1262-1272) testa só `casa.exemplo.br`.

### Severidades contestadas

- **Retorno pela lista sem fp esperada (afirmação 3 + furo de NAT que não acontece)**: o revisor propõe *bloqueante_1_0*. É o mecanismo da feature nº 1 do dono («persistência de servidores»), e no HEAD ele falha em silêncio justamente para quem mais precisa dele: anfitrião atrás de CGNAT sem UPnP. Sem `impressao_esperada` não sai LEVE, nem na entrada nem na reconexão (enlace.rs:994-1001, :2707-2715). Também deixa qualquer pessoa que já recebeu um link se passar pelo servidor para quem volta pela lista enquanto o anfitrião está fora, e mostra «PRIMEIRO CONTATO» a cada troca de porta, o que ensina a ignorar o aviso. O conserto é pequeno: em main.rs:1079 passar `esperada.or(impressao_guardada)` e testar lista→quarto→conexão.
- **Assinatura sobre nonce cru, sem vínculo com o canal TLS (repasse de identidade)**: o revisor propõe *bloqueante_1_0*. O analista incluiu isso no item (0) sem marcar como bloqueante, e o item (0) não resolve. Qualquer operador de servidor visitado pode entrar como o visitante em outro servidor SEELE onde a chave dele é admitida, inclusive como dono. A identidade é uma por máquina (seele-ffi/src/lib.rs:246-254). Exige subir o protocolo, e a v0.15 já quebrou a compatibilidade (protocolo 8), então sai mais barato quebrar de novo antes da 1.0 do que depois.
- **Conferir a fp dentro do TLS (token de uso único entregue antes da conferência)**: o revisor propõe *importante*. É real, mas o dano é limitado: o token vale uma vez e por 7 dias, e o ataque exige estar no caminho ou controlar o DNS. O link com nome põe o DNS como primeiro candidato, o que aumenta a exposição, mas não prejudica gravemente o usuário comum.
- **Nome público que não é host válido gera link que o outro lado recusa inteiro**: o revisor propõe *importante*. É falha silenciosa do lado de quem hospeda: o link sai, e quem recebe vê «inválido» dias depois. Consertar custa uma linha: chamar `separar` em `conferir_nome_publico` ou reanalisar o link em `convite_com_nome`.
- **seele:// clicável (registro no SO + instância única)**: o revisor propõe *importante*. Concordo que não bloqueia: copiar e colar funciona. O analista, porém, põe isso no escopo da 1.0 como se --entrar já servisse de base, e ele conecta sem perguntar (tela-boot.js:579-582). Só faz sentido depois de consertar a afirmação 2, senão vira entrega de token e assinatura com um clique.
- **Página https://seele.app.br/e**: o revisor propõe *desejavel*. Depende de trabalho fora deste repositório (site e painel da Cloudflare). A pré-condição de segurança (sem beacon injetado, com CSP) hoje não vale no site publicado (MEDIDO).

### O que o analista não viu

- O retorno pela lista nunca fura o NAT. Sem fp esperada, `Batida::preparar` devolve None (encontro.rs:93-99), porque main.rs:1079 só passa a fp do convite colado. Vale para a entrada (enlace.rs:994-1001; chegada.rs:525-528, :664-668) e para a reconexão (enlace.rs:2707-2715). O ADR 0047 §2.1 descreve esse caminho como funcionando, mas o único teste (seele-conformance/tests/quarto.rs:42) cobre só `onde_mora`. LIDO, não medido em rede real.
- Repasse de identidade: o cliente assina qualquer nonce cru (client.rs:1684-1690) e o servidor verifica sem vínculo com o canal (session.rs:595). Um operador malicioso entra como o visitante em outro servidor, inclusive como dono. Conferir a fp dentro do TLS não fecha esse ataque; é preciso material exportado do TLS na assinatura. Nenhum doc trata disso: grep por channel binding/exporter em specs/ e docs/ não acha nada.
- O quarto é tomável: a marca é `fp[..16]`, presente em todo link. Com o anfitrião fora do ar por mais de 60 s, qualquer pessoa pode registrar a marca e, reavivando a cada 15 s, impedir que o anfitrião volte (seele-encontro/src/lib.rs:73 e :118-125). A justificativa de segurança escrita em seele-proto/src/encontro.rs:65-67 e :84-88 contradiz main.rs:1011-1015, que põe a resposta do quarto na frente dos endereços guardados.
- Rastreamento: qualquer pessoa que já teve um link, inclusive um membro removido, pode perguntar QUEM e obter o IP atual do anfitrião enquanto a fp não mudar, e ela é estável entre reinícios (ADR 0047:68). Remover alguém da portaria não revoga isso (seele-encontro/src/lib.rs:288-293). É o custo de privacidade da persistência de URL e não aparece na análise.
- O link com nome quebra INTEIRO com um nome sintaticamente aceito, como `2001:db8::1`, `casa:abc` ou `x:99999`: uri.rs:1187-1193 aceita, uri.rs:582-595 recusa do outro lado. O guarda `o_que_passa_daqui_analisa_como_link` (uri.rs:1262-1272) cobre um único caso feliz.
- --entrar conecta sem confirmação (tela-boot.js:579-582), e o `Abertura` descarta um `seele://` posicional (main.rs:291), que é justamente o formato em que Windows e Linux entregam a URL. Reaproveitar --entrar como handler do SO violaria pendencias.md:427-431.
- MEDIDO no site publicado: seele.app.br tem o beacon do Cloudflare Web Analytics injetado, não manda cabeçalho CSP, e o fallback de SPA devolve 200 para qualquer rota, então /e hoje serve a landing. O código do site não está neste repositório.
- O pino chaveado por `host:porta` faz cada porta nova de um mesmo servidor virar um pino novo. Atrás de CGNAT o mesmo IP:porta pode ser reaproveitado por outro anfitrião, e aí o cliente recebe `Changed`, que é recusa dura no TLS (tofu.rs:278-283): o falso positivo mais alarmante do sistema, e ele fica mais provável quando o quarto entrega portas voláteis. INFERIDO.

### Desenho proposto pelo analista

## As quatro opções, contra a filosofia (ADR 0022: nenhum serviço no meio vê conteúdo; o ponto de encontro é trocável e "não consegue se passar por ninguém")

| | (1) página https com fragmento | (2) código curto resolvido pelo encontro | (3) DDNS documentado | (4) nome legível assinado no encontro |
|---|---|---|---|---|
| custo | M: página estática + aceitar a forma no `analisar` + UI | G | P: já existe; faltam os consertos L4-L9 | GG |
| estado/infra nova | nenhuma (seele.app.br já no ar via Cloudflare, MEDIDO) | tabela **durável** código→fp→endereços no encontro, em disco | nenhuma nossa (DNS do usuário) | registro durável de nomes, com política de posse, expiração, disputa e abuso |
| privacidade | quem hospeda a página vê IP, UA, hora e Referer do clique; **nunca o fragmento** | encontro sabe código↔fp↔IP de forma durável e quem procura quem | só o provedor de DNS do usuário | encontro sabe nome↔fp↔IP de forma durável e quem procura |
| o que o encontro passa a saber | nada | muito mais que o quarto de 60 s | nada | tudo que (2) sabe, mais o nome |
| identidade | a `fp` viaja inteira no fragmento | código aleatório: a `fp` vem do encontro, então **o operador pode se passar pelo servidor**; só é seguro se o código for autocertificante (≥100 bits da `fp`) | a `fp` continua no link | a assinatura prova quem registrou, não que é o servidor que o convidado quer; o operador vira autoridade de nomes |
| 0.15 | não lê `https://` (uri.rs:414), mas a página oferece o `seele://` para copiar | recusa `@código` (uri.rs:586-590) | já lê (desde a v0.11.0) | não lê |
| veredito | **1.0** | **não**; a variante autocertificante é a 1.x | **1.0, com consertos** | **não** |

## Recomendação para a 1.0: "amigável" = clicável, abre o app e a `fp` manda

### Formato
- **Link para mandar (novo, padrão na tela):**
  `https://seele.app.br/e#<corpo>`, onde `<corpo>` é **exatamente** o que vem depois de `seele://` no link de hoje, com `[`→`%5B` e `]`→`%5D`. Linkificadores de chat costumam parar em colchete, e o corte levaria a `fp` embora.
  - Exemplo: `https://seele.app.br/e#192.168.0.7:8383?alt=189.45.123.200:41877,%5B2804:14c::7c8d%5D:8383&enc=encontro.seele.app.br:8384/189.45.123.200:52011&fp=782c…&convite=2QKP…&v=1.0.0`
  - Com nome: `https://seele.app.br/e#casa.duckdns.org:8383?alt=192.168.0.7:8383,…&fp=…`
  - **Regra nova só desta forma:** `fp` obrigatória. Sem ela, "CHEGOU CORTADO". A gramática do ADR 0006 não muda.
- **Link direto (continua, segundo botão "copiar link direto"):** `seele://…` sem mudança. É o caminho de quem não quer web nenhuma no meio, e o de clientes 0.15.

### Quem resolve o quê
| peça | faz | não faz |
|---|---|---|
| navegador + página `/e` | `location.replace("seele://" + decodificar(%5B/%5D))`; mostra "Abrir no SEELE", "Baixar" e "Copiar link direto" | não interpreta o convite (nada de segundo analisador), não busca nada, não redireciona para outro esquema (sem redirecionamento aberto) |
| SO | entrega `seele://…` ao app (esquema registrado) | — |
| app | `uri::analisar` (um parser só, que passa a aceitar o prefixo `https://seele.app.br/e#`); DNS de nomes (todos os A/AAAA); QUEM só para retorno; confirmação antes de conectar | — |
| ponto de encontro | **nada muda na 1.0** | não aprende nada novo com a página |
| host da página (Cloudflare) | vê o clique (metadado) | não vê o fragmento |

### Como a `fp` continua garantindo a identidade
Nome, número e quarto passam a ser **dicas de onde**. A `fp` é **quem**, e é conferida antes de qualquer segredo sair.
1. `TofuVerifier::new(store, pin_key, esperada)`. No `FirstContact` com `esperada` que não bate, erro de TLS: nenhum `Hello`, token ou assinatura sai. Mantém a tabela de `verdict` para `Matches` (InviteDisagrees segue avisando e entrando).
2. No retorno pela lista: `expected_fingerprint = esperada.or(impressao_guardada)`.
3. Qualquer código curto futuro precisa ser **derivado da `fp`** (≥100 bits) e conferido contra o certificado. Código aleatório resolvido por terceiro fica proibido por escrito no ADR 0006.

### Etapas ordenadas
- **E0 (P, primeiro, independente) — a `fp` antes do segredo.** Os itens 1 e 2 acima.
  - Teste de conformidade: servidor real com `segredo: Some(token)` e `fp` errada. Exigir que o servidor **não** receba `Hello` e que o token continue válido.
  - Teste de retorno pela lista com servidor que mudou de porta, exigindo `Known` ou `FirstContactVerified`.
  - Prova de reversão: tirar a linha e ver os dois reprovarem.
  - Reusa `tofu.rs:113-146`, `seele-conformance/tests/convite.rs` e `quarto.rs`.
- **E1 (P) — consertar o link com nome.**
  - Guardar o nome em `Hospedagem` (ou na `Session`) e montar `convite_com_token` a partir de `convite_com_nome`.
  - Campo `nome_publico` em `servidores.json` (serde default), preenchido ao hospedar o guardado.
  - `conferir_nome_publico` só aprova o que `analisar` aceita.
  - Passar por `com_alternativos` (limite).
  - Todos os endereços do nome viram candidatos.
  - Porta externa quando `porta_nao_canonica`.
  - Aviso quando o degrau é furo ou só rede local, e quando o nome resolve para outro IP que não o público conhecido.
  - `seeled convite --nome`.
- **E2 (M) — esquema clicável** (fecha a pendência 10 e aceita o ADR 0006 alt. 3).
  - `tauri-plugin-deep-link` com `plugins.deep-link.desktop.schemes = ["seele"]`: o bundler põe `CFBundleURLTypes` no macOS e `MimeType` no .deb.
  - `tauri-plugin-single-instance` (feature `deep-link`); precisa passar no `deny.toml`. Alternativa sem dependência: `RunEvent::Opened` mais um soquete local próprio.
  - Windows: `seele-instalador/src/registro.rs` escreve `HKLM\Software\Classes\seele` (`URL Protocol`, `shell\open\command "…\seele-app.exe" "%1"`) e `desinstalar.rs` apaga. A opção "tratar seele://" volta ao passo 02 do instalador (janela.rs:271-275).
  - `Abertura` aceita `seele://…` solto em argv.
  - Janela aberta: evento para a tela "ENTRAR EM <alvo>?". A tela mostra se o link traz `fp`, **nunca conecta sozinha** e não gasta o convite sem clique.
  - Com sessão ou hospedagem ativa: "sair de X (derruba o servidor que você hospeda) e entrar em Y?". Multiconexão não existe (main.rs:899).
  - `v=` diferente e instalado: reusa `abrir_versao` com o link inteiro.
- **E3 (M, depende de E2 para "abrir"; sem E2 a página só copia e baixa) — a página e a forma https.**
  - Página estática sem script de terceiros: CSP `default-src 'none'` com hash do script inline, `<meta name="referrer" content="no-referrer">`, `noindex`, OG tags fixas para a prévia ("Convite para um servidor SEELE").
  - Desligar analytics e injeções do Cloudflare nessa rota.
  - Teste no repositório de que a página não faz `fetch`/XHR nem carrega recurso externo.
  - `uri.rs`: constante `PREFIXO_WEB` e a decodificação de `%5B`/`%5D` **dentro** de `analisar`.
  - A tela mostra o https como link principal.
  - Página de doc: "o que seele.app.br vê quando alguém clica".
- **E4 (1.x, G, depende da frente de persistência) — link curto e estável autocertificante.**
  - Forma: `https://seele.app.br/e#@<20 caracteres base32 da fp>`, mais o ponto quando não for o padrão.
  - Resolvido pelo QUEM, que já existe, com marca ≥100 bits.
  - Exige: anfitrião sempre fazendo MORO; MORO assinado pela chave do servidor (sem isso, ocupar a marca vira negação de serviço); dica de LAN para quem está na mesma casa.
  - O cliente confere o prefixo contra o certificado.

### Compatibilidade com 0.15
- A gramática `seele://` não muda, então um 0.15 lê o link direto.
- A forma https é nova: um 0.15 copia da página.
- Se a 1.0 mantiver `PROTOCOL_VERSION` 8, 0.15 e 1.0 conversam. Se subir, o `v=` e o launcher (ADR 0046) cobrem quem tem a versão instalada.

### O que se reusa
`uri::analisar` como único analisador; `--entrar`, `cumprirAAbertura` e `abrir_versao`; `analisar_convite` e a `Session.convite`; a tabela `verdict`; os testes `convite.rs` e `quarto.rs`; o instalador próprio (registro.rs); o domínio seele.app.br.

### Perguntas ao dono

- Por 'URL amigável' você quer (a) um link que se clica e abre o app, (b) um link curto e que não muda, ou (c) um nome legível como casa-do-alexandre? A recomendação entrega (a) na 1.0, (c) via DNS do próprio usuário e deixa (b) para a 1.x, junto da frente de persistência.
- Você aceita que o host da página (hoje o Cloudflare de seele.app.br) veja o metadado do clique (IP, navegador, hora, app de origem), nunca o conteúdo do convite? Ou prefere hospedar /e em outro lugar (GitHub Pages do SEELE-RELEASES)? Algum analytics ou injeção do Cloudflare está ligado nesse domínio?
- Qual link a tela mostra primeiro: o https (clicável em qualquer conversa) ou o seele:// direto (nenhuma web no meio)?
- A 1.0 mantém o PROTOCOL_VERSION 8 (conversa com 0.15) ou sobe? Isso define se o 'copiar link direto' da página serve a quem ainda está na 0.15.
- No Windows, registrar seele:// por máquina no instalador (HKLM, exige a etapa de instalação) ou por usuário quando o app abre (HKCU, cobre também quem não reinstala)?
- Clicar num convite com uma sessão aberta, ou hospedando, deve oferecer 'sair e entrar no outro' (derrubando o servidor hospedado) ou só avisar e não fazer nada até existir multiconexão?
- Topa uma mudança de protocolo para vincular a assinatura do handshake ao canal TLS (fecha o repasse de identidade), ou fica só com a conferência da fp antes do Hello na 1.0?

---

## Conexão simultânea

**Resumo do analista.** Nada da multiconexão foi construído no HEAD e2fac4d, que é a v0.15.0 publicada. Dos três passos do ADR 0047 §3.4: o evento com dono existe só pela metade, no Rust, e com semântica de sessão única. O áudio continua abrindo na conexão, e não existe camada de sessões. Desde a estimativa do 0031, a superfície que a multiconexão teria de tocar mais que triplicou: são 152 comandos Tauri, 61 resoluções de `.connection()` em 60 comandos, 8 ouvintes no canal de eventos, 34 campos no Snapshot e cerca de 62 variáveis de módulo que guardam estado de sessão. O ciclo de vida dos MODs (geração E2, tema em `#tela-sessao`) foi construído supondo uma sessão só. Com isso, tirar o `AlreadyConnected` sem mexer na Bridge deixaria a sessão anterior sem receber eventos, sem aviso nenhum. Recomendação: a multiconexão fica para a 1.1, no recorte (b): conexões quentes em segundo plano, voz só na sessão da frente e placa com não lidas na trilha. Na 1.0 entram duas coisas baratas: o anfitrião poder visitar outro servidor sem derrubar o próprio (reaproveita o `desmontar_a_sessao`, que já existe) e a correção de dois vazamentos de estado entre servidores que já acontecem hoje (rascunhos e volumes).

### O que existe

- `construido_e_publicado`: Guarda de uma sessão por vez (AlreadyConnected) (apps/seele-app/src/main.rs:898-900)
- `construido_e_publicado`: Troca de servidor pela trilha (desconecta, passa pela entrada e conecta, com confirmação que nomeia os dois) (apps/seele-app/ui/tela-sessao.js:3529-3597; tests/frontend.rs:3094)
- `parcial`: Evento com dono (passo 1 do 0047): Bridge com geração, descarte e contagem no Rust (apps/seele-app/src/main.rs:791-834, 1109-1114; payload cru em :826)
- `ausente`: Áudio abrindo na entrada da sala (passo 2 do 0047) (crates/seele-ffi/src/lib.rs:3930-3955 abre na conexão; tela-boot.js:198 audio:true)
- `ausente`: Camada de sessões acima do Snapshot (passo 3 do 0047) (apps/seele-app/src/main.rs:140, 359-365 (slot único))
- `construido_e_publicado`: Voice::reopen e a lista única de controles que atravessam a reabertura (crates/seele-core/src/voice.rs:898, 1226, 1243; usado em crates/seele-ffi/src/lib.rs:4113)
- `construido_e_publicado`: desmontar_a_sessao: sai da sessão e mantém a hospedagem de pé (apps/seele-app/src/main.rs:1999-2003; ui/tela-fim.js:171-172)
- `construido_mas_nao_ligado`: Não lidas por canal e marca de lido no core (crates/seele-core/src/state.rs:741 (nao_lidas), :843 (marcar_lido))
- `construido_mas_nao_ligado`: Identidade lógica do servidor (instância) (crates/seele-server/src/persistence/instancia.rs; chega a crates/seele-core/src/state.rs:266 e não ao Snapshot)
- `construido_e_publicado`: Geração passada pela janela em comandos (token de sessão por comando) (13 comandos com geracao: u64 em main.rs; base.js:468-476)
- `parcial`: Mapa por sessão para rascunhos e volumes (o comentário promete «por sessão») (apps/seele-app/ui/tela-sessao.js:143, 2580; não há limpeza em ejetar())
- `parcial`: Despedida de todas as conexões ao fechar a janela (apps/seele-app/src/main.rs:8144-8152 (só a da frente))
- `ausente`: Placa por servidor na trilha com estado ou não lidas (trilha lista só o histórico (index.html:905-966; conhecidosDaTrilha em tela-sessao.js:109))
- `so_desenho`: ADR da multiconexão (docs/adr/0031 (proposto, nada construído); docs/adr/0047 §3 (rascunho))

### Lacunas

- **[importante · esforço G · risco medio] Multiconexão não existe: estar em B custa a conversa em A.** Não dá para receber texto ou presença de dois servidores. A trilha troca de servidor (desconecta e conecta). Não é bloqueante porque a troca existe, é de um clique, e a confirmação diz o preço. O pedido do dono é por conveniência, não por uma falha que prejudique o usuário.
  - *Evidência:* main.rs:898-900 (AlreadyConnected); tela-sessao.js:3490-3499 («Dá para estar em um servidor por vez»); main.rs:140 (slot único)
- **[importante · esforço M · risco medio] O anfitrião não pode visitar outro servidor sem derrubar o próprio para todo mundo.** Quem hospeda pelo app e aperta outro servidor na trilha derruba o próprio servidor: ejetar() chama disconnect, e disconnect encerra a Hospedagem. O verbo que não derruba (desmontar_a_sessao) existe e só é usado pelo RECONECTAR. É a dor mais concreta que «estar em dois lugares» resolve, e custa uma fração da multiconexão.
  - *Evidência:* tela-sessao.js (ejetar: invoke("disconnect")); main.rs:2005-2027; main.rs:1999-2003; tela-sessao.js:3494-3498
- **[importante · esforço P · risco baixo] Rascunho e volume vazam de um servidor para outro já hoje.** rascunhos é chaveado pelo id do canal, que é INTEGER PRIMARY KEY por servidor, e nunca é limpo. O texto escrito no canal 1 de A reaparece no campo ao abrir o canal 1 de B depois de trocar pela trilha, e pode ser enviado ao grupo errado. volumes, chaveado por apelido, mostra no controle deslizante o valor de outra sessão enquanto o ganho real é 100%. O comentário promete «por sessão», e o código não cumpre: existir não é funcionar. Efeito INFERIDO pela leitura, não reproduzido.
  - *Evidência:* tela-sessao.js:2580-2595, 3355-3357 (restaurarRascunho ao clicar no canal); tela-sessao.js:143, 2292, 4225; nenhuma limpeza em ejetar(); schema.rs:93-94
- **[importante · esforço P · risco baixo] Tirar o guarda AlreadyConnected deixaria a sessão anterior sem eventos, sem aviso nenhum.** A geração E2 é global: connect revoga antes de conectar, e a Bridge compara com um de_pe único. Com duas conexões, a primeira passa a descartar todos os eventos (ModReply, TransferChanged, Ended), contados num número que só a bancada de MODs lê. O laço de snapshot de 500 ms mascara a falha. O Ended de uma sessão em segundo plano revogaria a da frente. Hoje não afeta ninguém. Vira defeito publicado no dia em que alguém fizer a multiconexão pelo caminho curto.
  - *Evidência:* main.rs:913 (revogar em connect); main.rs:802-806 e 827-833 (Bridge); main.rs:1109-1114; main.rs:2298 e camada-mods.js:1137; tela-sessao.js:4373-4375
- **[importante · esforço G · risco alto] O microfone abre ao conectar, não ao entrar numa sala de voz.** Conectar só para ler texto abre dois cpal::Stream. O indicador de microfone do sistema acende (INFERIDO), e N conexões abririam N. Mudar isso exige tirar mudo, isolamento e modo de dentro de Voice. Senão o mudo se perde ao fechar o caminho, e o roster e o portão local passam a discordar. Isso mexe no caminho de voz perto da 1.0, e a voz por microfone real entre duas máquinas ainda não foi verificada (README:288).
  - *Evidência:* crates/seele-ffi/src/lib.rs:3930-3955; tela-boot.js:195-198; lib.rs:2948-2970, 3045-3070 (controles dentro de Voice); lib.rs:4089-4112
- **[importante · esforço M · risco medio] O ciclo de vida dos MODs foi desenhado para uma sessão só, e nenhum ADR cruza isso com o 0031.** Há um tema só em #tela-sessao, uma geracaoDaSessao só no JS e um mods_nativos por geração. O contrato E2 fala de sessão e geração, mas a implementação juntou as duas num número. Falta decidir que MOD de servidor em segundo plano não roda, e que trazer para a frente é montar de novo, o que o §5 do contrato já trata como encerramento na troca de servidor.
  - *Evidência:* base.js:468, 1562-1571, 2147-2157; main.rs:87-110; docs/contrato-mods-api-propria-para-claude.md:45, 109; grep por 0031 e multiconexão nos docs de MODs vazio
- **[desejável · esforço M · risco baixo] Não lidas: o core conta e a tela não mostra.** Room::nao_lidas e marcar_lido existem desde a v0.15 e não passam pela FFI. marcar_lido nunca é chamado, então expor a contagem direto contaria a página de histórico inteira. É a base da placa de qualquer multiconexão, e já serviria hoje para os canais da sessão única.
  - *Evidência:* crates/seele-core/src/state.rs:741, 843; grep por nao_lidas em crates/seele-ffi, apps/ vazio; session.rs:4750-4753 (só canais abertos recebem)
- **[desejável · esforço P · risco baixo] O fechamento da janela despede uma conexão só, e o guarda continuaria verde com N.** Só pesa quando houver várias conexões: as outras ficariam sentadas na sala até o tempo limite de ociosidade, que é o defeito que tests/encerramento.rs existe para impedir. O guarda confere texto e não pegaria isso.
  - *Evidência:* main.rs:8122, 8144-8152; tests/encerramento.rs (fechar_a_janela_avisa_o_servidor)
- **[desejável · esforço P · risco medio] Teto de vídeo e consentimento de par são por conexão, e a subida é da máquina.** Com várias conexões, cada uma aplicaria 60% do caminho que mediu e declararia empréstimo de subida. Uma conexão em segundo plano precisa de assistir(false), parar de compartilhar e consentimento zero.
  - *Evidência:* crates/seele-core/src/tela.rs:18-35, 80; main.rs:1175-1193
- **[desejável · esforço P · risco baixo] A pendência 24 e o 0031 descrevem o produto de 18/08.** A pendência 24 cita o + desabilitado, linhas que mudaram e um teste que não existe mais. Quem for decidir a 1.0 lendo a pendência vai subestimar o custo em 3×.
  - *Evidência:* docs/pendencias.md:2951-3036; grep -c do nome do teste antigo em tests/frontend.rs = 0; index.html:964-966

### O que o revisor conferiu

- **CONFIRMADA**: 1. Nenhum dos três passos da multiconexão está pronto: evento com dono pela metade e com semântica de sessão única, áudio na entrada da sala ausente, camada de sessões ausente.
  - LIDO no HEAD e2fac4d: main.rs:826 `let _ = self.app.emit(EVENT_CHANNEL, &event);` emite o Event cru. O enum Event (crates/seele-ffi/src/types.rs:1440-1573: ModReply, RosterChanged, MessagesChanged, ..., ScreenClosed) não tem campo de sessão nem de servidor. main.rs:1108-1113: toda Bridge compara com `de_pe: Arc::clone(&session.geracao)`, que é um único AtomicU64 (main.rs:108). main.rs:140 guarda `connection: Mutex<Option<Arc<Connection>>>`, uma vaga só, e não existe mapa de Connection entre os campos de Session (main.rs:87-267). A voz abre dentro do driver da conexão, em `if config.audio { Voice::start_preferring(...) }` (seele-ffi/src/lib.rs:3930-3951), e aí o input.play() do cpal já roda (seele-audio/src/device.rs:783). enter_voice_room só manda um Command (lib.rs:1386-1392). tela-boot.js:198 manda `audio: true` sempre. **Correção:** Um detalhe: o ADR 0047 §3.4 lista cinco etapas (evento com dono, voz fora da Connection, camada de sessões, casca, trilha), não três. O analista avaliou as três de backend. A conclusão continua certa.
- **PARCIAL**: 2. A superfície que a multiconexão teria de tocar mais que triplicou desde o 0031 e cresceu 1,6× só na última semana.
  - MEDIDO com git show/grep. No 1e4676d (criação do 0031, 18/08): 51 comandos, 28 `.plug()`, 5 848 linhas de .js em ui/ e 2 ouvintes. No 3c59eec (15/09): 92 comandos, 47 `.connection()`, 12 210 linhas e 6 ouvintes. No e2fac4d: 152 comandos, 61 `.connection()` (em 60 corpos de comando), 22 898 linhas, 8 ouvintes de `seele://event` (base.js:480 e :2191, camada-compartilhar.js:685, tela-server.js:1473, tela-chamada.js:864, palco-imagem.js:518, tela-boot.js:283, tela-sessao.js:4331) e 34 campos em Snapshot (types.rs:1167). Os fatores ficam assim: comandos 2,98×, `.connection()` 2,2–2,65×, JS 3,9×, ouvintes 4× e Snapshot 1,4×. Na última semana: comandos 1,65×, `.connection()` 1,30×, JS 1,88× e ouvintes 1,33×. **Correção:** Só o JS e os ouvintes passaram de 3×. Os comandos ficaram em 2,98× e as resoluções de conexão, que são a medida que importa, em cerca de 2,5×. O Snapshot cresceu 1,4×. O «1,6× na semana» vale para comandos e JS, não para `.connection()` nem para ouvintes. E há uma contradição interna: pela afirmação 4 do próprio analista, os 60 comandos ficam intactos no recorte (b). Então o crescimento de comandos não mede o custo do que ele recomenda.
- **PARCIAL**: 3. Tirar o AlreadyConnected sem mudar a Bridge deixa a sessão anterior sem eventos, sem aviso nenhum, e o laço de 500 ms mascara a falha.
  - LIDO: connect chama `session.revogar()` (main.rs:913 → 320-333), e a Bridge descarta quando `geracao != de_pe` (main.rs:802-806). Mas se só o guarda sair, a linha `*slot = Some(connection);` (main.rs:1198) sobrescreve incondicionalmente o único Arc da conexão anterior. Não há outro clone longo: o grep por `Arc::clone(&conn|connection.clone()` em main.rs volta vazio. O Arc cai, e `impl Drop for Connection { fn drop(&mut self) { self.disconnect(); } }` (seele-ffi/src/lib.rs:3073-3077) encerra a sessão anterior. O revogar também já teria parado os MODs nativos daquela geração (revogar_em, main.rs:3518-3533). O contador de descartados aparece ao usuário na linha de diagnóstico da camada de MODs (camada-mods.js:1133-1160), mas sem alerta nenhum. O setInterval de 500 ms existe (tela-sessao.js:4373) e chama `snapshot`, que não confere geração (main.rs:2127-2133). **Correção:** No cenário literal («só tirar o guarda»), a sessão anterior não fica viva e surda. Ela é derrubada calada pelo Drop: o outro servidor vê a pessoa sair e a janela não recebe Ended. O modo «viva, sem eventos, mascarada pelo polling» é real em dois casos. O primeiro é uma refatoração para mapa de conexões que não mexa na Bridge. O segundo já existe hoje, por corrida no próprio guarda (ver omissões): o check em main.rs:898 e a escrita em :1198 são separados pelo `.await` do handshake (main.rs:1116), sem trava de «conectando».
- **PARCIAL**: 4. O recorte (b) mantém os 60 comandos intactos porque Session::connection() pode continuar devolvendo a sessão da frente; o custo fica concentrado em Bridge, connect, despedir_se, FFI de voz e trilha.
  - LIDO: Session::connection() está em main.rs:359-365, e os usos diretos do slot ficam em connect (main.rs:1197, 1260), em desmontar_o_cliente (via `.connection.lock()`) e em despedir_se (main.rs:8148). Aparecem contraexemplos no desenho (b). (i) 13 comandos de MOD recebem `geracao: u64` (mod_request, mod_nativo_*, codigo_do_mod, midia_do_mod, enviar_imagem_mod, escolher_para_o_mod etc.) e passam por confere_geracao contra um único atômico (main.rs:338-356). Um MOD de uma sessão em segundo plano seria recusado e contado como «comando de geração morta». (ii) connect chama revogar(), que para os MODs nativos e solta os arquivos escolhidos da geração anterior (main.rs:320-333, 3518-3533): conectar ao segundo servidor derruba os MODs do primeiro. (iii) Sair de um servidor pela placa dele, sem trazê-lo à frente, exige um disconnect com alvo. (iv) As não lidas por placa precisam de leitura por sessão, e nao_lidas (seele-core/src/state.rs:741) não chega ao FFI. (v) Session.busca, endereco_da_sessao, alvo e convite também são vagas únicas de sessão (main.rs:166, 202-226). **Correção:** Os cerca de 47 comandos «simples» podem mesmo continuar presos à frente. Mas a geração única, os 13 comandos de MOD que a carregam e o ciclo de vida dos MODs (revogar no connect) entram no custo do (b), a menos que se decida que MOD em segundo plano morre. A lista de custo concentrado também deixa de fora a busca e os demais campos de vaga única da Session.
- **CONFIRMADA**: 5. Tirar o áudio da conexão não é mover uma linha: mudo, isolamento e modo moram dentro de Voice, e sem Voice o Snapshot volta ao silêncio.
  - LIDO: audio_state lê mode, muted, total_isolation, níveis e aparelhos de `self.shared.voice` (seele-ffi/src/lib.rs:2948-2980). Sem Voice, devolve AudioState::silent() com PushToTalk, muted:false e isolation:false (lib.rs:3052-3070). set_muted e set_total_isolation só aplicam localmente se a Voice existir (lib.rs:2073-2094), e set_voice_mode e set_supressao_de_ruido viram no-op calado sem Voice (lib.rs:2412-2430). Não há cópia desses controles em Shared: o único lugar do estado é Controls, dentro de Voice (voice.rs:898 carregar_controles; :1226 reopen; :1243 carry_over). **Correção:** Um detalhe: set_muted também manda Command::SetMuted ao servidor (lib.rs:2079), então o mudo no roster sobrevive. O que se perde sem Voice é o portão local e o modo, e é exatamente o descompasso que o comentário em lib.rs:4089-4112 descreve.
- **PARCIAL**: 6. Não há rede automática que pegue uma regressão de casca: nada roda sozinho e os guardas da casca são textuais.
  - LIDO: ci.yml:24-28 tem push e pull_request comentados, e `on: workflow_dispatch` só em ci.yml:27-28 e release.yml:21-22. apps/seele-app/tests/frontend.rs tem 240 #[test] e 794 `assert!(`. São textuais: por exemplo, frontend.rs:3094 procura a substring "ejetar(" em trocarDeServidor. Mas existe execução de JS: apps/seele-app/bancada/ tem 15 bancadas Playwright que servem ui/ num Chromium com `window.__TAURI__` simulado (telas.cjs, cabeçalho; conversa-com-estado.cjs:49). Quatro delas estão ligadas no job `bancadas` do ci.yml:238-259. O release.yml não as roda: o `validar` executa só `cargo test --workspace` (release.yml:191) e ainda pode ser pulado pela entrada `sem_validar` (release.yml:40, 135). **Correção:** «Nada roda sozinho» está confirmado. Mas os guardas da casca não são só textuais: há um harness de runtime (Playwright com ponte simulada), que é o lugar natural para um teste de eventos de duas sessões. Ele não cobre sessões múltiplas, só roda por disparo manual e não faz parte do portão de publicação. Não consegui reproduzir o número «531»: medi 240 testes, 323 fn e 858 linhas com assert.

### Severidades contestadas

- **Despedida de todas as conexões ao fechar a janela (marcada 'parcial')**: o revisor propõe *nao_se_aplica_a_1_0 (desejavel, só quando houver multiconexão)*. No produto de sessão única que a 1.0 vai ter, despedir_se (main.rs:8144-8152) já despede a única conexão que existe. Chamar isso de 'parcial' faz parecer que falta algo publicado. Isso só vira lacuna depois que o slot virar mapa.
- **Anfitrião visitar outro servidor sem derrubar o próprio (proposto para a 1.0 como barato, só reaproveitando desmontar_a_sessao)**: o revisor propõe *desejavel (e não barato)*. Hoje não existe o estado «hospedando, mas fora do próprio servidor»: toda ida à entrada derruba a hospedagem (tela-sessao.js:3653 com ejetar→disconnect; tela-fim.js:141-143 com pararDeHospedar:true). Criar esse estado deixa um beco sem saída. hospedar devolve JaHospedando se a hospedagem está de pé (main.rs:1531-1537; frases.js:689 «JÁ ESTOU HOSPEDANDO NESTA JANELA»). O servidor local não entra na lista de conhecidos (main.rs:1216, `if !hospedado_aqui`), e a trilha só o desenha quando é o atual (index.html:907-915). Então o anfitrião não tem por onde voltar ao próprio servidor. Também falta um indicador de que a máquina segue hospedando gente enquanto a pessoa está noutro lugar, e é preciso reescrever a frase de consequência (tela-sessao.js:3490-3499) e o guarda textual (frontend.rs:3094). A troca já avisa «ele cai junto», então hoje o custo é dito, não silencioso.
- **Mapa por sessão para rascunhos e volumes (marcada 'parcial', vazamento entre servidores)**: o revisor propõe *importante (entra na 1.0, e o volume é mais grave do que foi descrito)*. Rascunhos: o Map é indexado pelo id do canal (tela-sessao.js:2580-2594), e esse id é INTEGER PRIMARY KEY por banco de servidor (seele-server/src/persistence/schema.rs:51; o canal 1 é 'geral' em todo servidor). O rascunho escrito na #geral de A aparece no campo da #geral de B e sai com um Enter. É vazamento de texto privado entre servidores, visível no campo, mas fácil de mandar sem querer. Volumes: o ganho real é por SSRC dentro da Voice (lib.rs:2468-2485), e a Voice nasce nova a cada connect. O Map da tela é por apelido (tela-sessao.js:143, 2292, 4225), nunca é reaplicado e o Snapshot não traz o ganho por pessoa. Depois de qualquer reconexão, até ao mesmo servidor, o deslizante mostra X% e o áudio sai a 100%. É «o produto sabe e não conta», não só vazamento entre servidores.
- **Áudio abrindo na entrada da sala (marcado 'ausente' só como passo 2 da multiconexão)**: o revisor propõe *importante, independente da multiconexão*. LIDO: o microfone abre e roda no connect (lib.rs:3930-3951; device.rs:783 input.play()), com audio:true fixo (tela-boot.js:198). Quem entra só para ler texto fica com o indicador de microfone do sistema aceso a sessão inteira. O ADR 0031 (linhas ~198-206), o ADR 0047 §3.3 e o pendencias §24 (item 1) chamam isso de melhoria de privacidade «por si só». Para um produto com PTT como padrão por segurança, é item de 1.0 mesmo com a multiconexão adiada. Não é bloqueante.

### O que o analista não viu

- Corrida no próprio guarda AlreadyConnected (LIDO, alcance INFERIDO). O connect confere `session.connection().is_ok()` (main.rs:898), revoga (:913), espera o handshake em `spawn_blocking(...).await` (:1116) e só então escreve `*slot = Some(connection)` sem conferir de novo (:1198). Não há trava de «conectando» na Session. Se dois `connect` correm juntos, o último a terminar vence o slot e a outra conexão cai pelo Drop. Se o vencedor for a tentativa de geração mais antiga, a Bridge dele descarta todos os eventos (gen ≠ de_pe), os comandos de MOD são recusados e a tela continua desenhando pelo polling de 500 ms. É o modo de falha da afirmação 3, possível no código publicado. Na casca, o único freio é `botao-conectar.disabled` (tela-boot.js:~175); conectar() é chamado também pela trilha, pelo diálogo de servidores, pelo aceite de MODs e pelo RECONECTAR.
- Já existe multiconexão de fato, entre processos, e sem coordenação do microfone (LIDO, comportamento em execução INFERIDO). O launcher do ADR 0046 abre outra versão «sem fechar esta janela» (main.rs:1443). camada-servidores.js:187-190 abre na versão do servidor quando o link a traz, e tela-boot.js:549/981 e camada-versoes.js:96 também abrem. Não há plugin de instância única (grep vazio em Cargo.toml, tauri.conf.json e main.rs). Cada processo abre o próprio microfone no connect. O resultado são duas sessões em dois servidores, dois fluxos de captura e o mesmo config_dir (conhecidos, servidores.json e preferências escritos por dois processos): exatamente a «voz concorrente» que o ADR 0031 proíbe. Com a quebra de protocolo 7→8 da 0.15, o launcher é o caminho para falar com servidores 0.14, então esse caso tende a crescer. Qualquer desenho de multiconexão em um processo precisa dizer como convive com o ADR 0046, porque um servidor de outra versão é, por construção, outro processo.
- A chamada privada depende da multiconexão «para ser boa» (ADR 0047 §4.3, bloco «Onde ela escuta»), e o analista não cruzou as frentes. Com a multiconexão na 1.1, uma chamada privada na 1.0 tira a pessoa do servidor em que ela está. E como Session.hospedagem é vaga única (main.rs:145) e a porta é fixa (PORTA_PADRAO 8383, main.rs:1981), quem hospeda não consegue ligar sem derrubar o próprio servidor. O recorte (b), com voz só na sessão da frente, derrubaria a chamada privada sempre que a pessoa olhasse um servidor. Isso precisa entrar na decisão de escopo da 1.0.
- O recorte (b) contraria a decisão escrita no ADR 0031: «A exclusividade é do microfone, e por isso o escopo dela é a sala de voz — não o servidor»; «escrever num canal de qualquer uma delas funciona sem trocar nada de lugar» (0031, seção 'Então o caminho de voz sai da Connection'). Com a voz só na frente, perde-se o caso de uso central que motiva o pedido do dono («+ para se conectar a vários ao mesmo tempo»): estar em voz em A lendo B. A escolha entre (a) e (b) é do dono e deveria ser apresentada assim, não como o recorte natural.
- Docs desatualizados sobre a frente. O pendencias §24 (docs/pendencias.md:2951-2990) ainda diz que o `+` está desabilitado e cita main.rs:49 e 168-170. O ADR 0031 cita main.rs:49 e 168-170, e o 0047 cita main.rs:57-58 e :191, quando o HEAD tem :140 e :899. Quem pegar a frente na 1.1 parte de um retrato errado.
- Instância lógica do servidor como chave (LIDO): room.instancia (seele-core/src/state.rs:266, gravada em :1039) não é lida por ninguém fora de state.rs. É a peça que resolveria os vazamentos de rascunho e volume por servidor sem depender do endereço, e também o índice natural de uma futura camada de sessões. O analista a lista como 'construído mas não ligado', mas não a liga à correção que ele mesmo propõe para a 1.0.

### Desenho proposto pelo analista

## Recomendação: a multiconexão fica para a 1.1, no recorte (b). A 1.0 leva só o que é barato, útil sozinho e prepara o terreno

### Justificativa em código
1. **O custo triplicou e continua subindo.** São 152 comandos e 61 `.connection()` em 60 comandos (eram 51 e 23 na estimativa do 0031). São 8 ouvintes que não sabem de qual sessão vem o evento e cerca de 62 variáveis de módulo com estado de sessão. Só na última semana foram +60 comandos, puxados por MODs.
2. **A infraestrutura recente supõe uma sessão só.** A geração E2 é um `AtomicU64` global (main.rs:1109-1114) e um `let` global (base.js:468). O tema vai em `#tela-sessao`. O caminho curto (tirar o `AlreadyConnected`) produz a falha que o 0031 chama de mais cara, e sem aviso nenhum (main.rs:913 + 802-806).
3. **Não há rede de segurança automática.** O CI só roda por `workflow_dispatch` (ci.yml:24-28), e os 531 guardas da casca são asserções textuais. Uma refatoração de casca desse tamanho perto da 1.0 seria testada no olho.
4. **Nenhum usuário da 1.0 é prejudicado de forma grave sem ela.** A troca existe e é honesta (tela-sessao.js:3490-3512).

### Alternativas, com o que muda em cada camada

| | seele-core | seele-ffi | app (Rust + casca) | risco | esforço (INFERIDO) |
|---|---|---|---|---|---|
| **(d) visitar sem derrubar a própria hospedagem** | nada | nada | trocar usa `desmontar_a_sessao` quando a sessão é a hospedada; entrada «seu servidor, no ar» na trilha a partir de `estado_da_porta`; controle explícito PARAR DE HOSPEDAR (hoje parar = SAIR); consertar `renomear_server`/`servidor_no_ar` (main.rs:2852-2862) e `hospedandoAqui()` para comparar com `endereco_da_sessao` | baixo-médio | **M** (2–4 dias) |
| **(b) conexões quentes, voz só na frente, sem casca plural** | nada (Room já é por conexão; `nao_lidas` existe) | `soltar_voz()` guardando os controles (reusa `carregar_controles`, voice.rs:898) e `abrir_voz()` com a mídia e o ssrc lembrados; `nao_lidas()` somando os canais; ligar `marcar_lido` | `Sessoes{frente, vivas}`; **`Session::connection()` continua devolvendo a frente, então os 60 comandos não mudam**; Bridge com id de sessão, placa para as de segundo plano, `Ended` de fundo sem revogar a frente; `despedir_se` em laço; teto de sessões. Casca: trilha com placa, troca = o `ejetar()` sem `disconnect` + `entrarNaGeracao` + `desenhar` | médio | **G** (1,5–2,5 semanas) |
| **(a) secundárias de texto; a voz fica na sala enquanto a frente muda (0031 inteiro)** | nada | tudo de (b) + áudio aberto na entrada da sala | tudo de (b) + rotear 12 comandos de voz (set_talking, set_muted, set_total_isolation, set_voice_mode, escolher_microfone, escolher_saida, controles_da_voz, ajustar_sensibilidade_da_voz, ajustar_reducao_de_ruido, set_volume, enter/leave_voice_room) à «sessão da voz» + juntar os 9 campos de áudio no Snapshot da frente + indicador «voz em X» + 12 comandos de tela | alto | **GG** (3–4 semanas) |
| **(c) placa por reconexão periódica** | persistir `lidas_ate` por servidor (arquivo novo) | nada | temporizador | **social alto**: cada volta transmite `PersonPresent`/`PersonGone` a todos (seele-proto control.rs:2031-2069), paga os 500 ms do quarto (lib.rs:7780), passa pelo portão de MODs e pela portaria. **Recusar.** | M |

### Na 1.0 (ordem por risco)
1. **Higiene de estado da casca (P).** `ejetar()` limpa `rascunhos`, `volumes`, `previas`, `previasFechadas` e `previasDeLink`, mais um guarda textual que liste todo `const … = new Map()` de tela-sessao.js e exija limpeza. Prova contra a regressão: reverter a limpeza e ver o guarda reprovar.
2. **(d) Visitar sem derrubar (M).** É a entrega que a 1.0 pode anunciar em vez de «conexão simultânea». O servidor da pessoa continua no ar e a portaria continua contando (camada-portaria.js:565 já consulta a cada 5 s, independente da sessão).
3. **Trancar a armadilha (P).** Um comentário e um teste junto do `AlreadyConnected` dizendo que ele só pode sair junto com o id de sessão na Bridge.
4. **Atualizar a pendência 24 e o status do 0031** com os números deste relatório (P).

### Na 1.1: (b), em cinco etapas com dependências
1. **Evento com dono** (sem dependência; faz sentido sozinho). A Bridge ganha um `sessao: u64` que nunca se repete (reusa o contador de geração como identidade de sessão) e o `frente: Arc<AtomicU64>`. O payload vira `{sessao, evento}`, e os 8 ouvintes descartam por `daGeracaoDePe` (base.js:474). Teste de conformidade com **duas `Connection` contra um servidor local** provando que evento de B nunca chega à janela como se fosse de A. Provar revertendo.
2. **Voz sob demanda na FFI** (depende de 1 para o aviso saber de quem é). Só `soltar_voz`/`abrir_voz`, com os controles guardados. **Sem** mudar ainda quando o áudio abre.
3. **Camada de sessões no app** (depende de 1 e 2). Ao ir para segundo plano: `leave_voice_room`, `soltar_voz`, `assistir(false)` em tudo, `parar_de_compartilhar`, consentimento de par (0,false) e `encerrarOAmbienteDosMods`. `alvo`, `endereco_da_sessao`, `convite` e `busca` passam para `SessaoViva`.
4. **Placa** (depende de 3). Não lidas só dos canais já abertos, mais o enlace vermelho na placa (0031: «estado vence identidade»). Abrir todos os canais na entrada custa uma página de histórico cada; fica decidido na etapa, não suposto.
5. **Casca** (depende de 3). Trilha com as sessões vivas, e trocar sem passar pela entrada. MODs remontados pelo caminho de entrada que já existe (`entrarNaGeracao` + `carregarMods`).

### Na 1.2: (a)
Voz que fica na sala enquanto a frente muda, áudio aberto na entrada da sala (passo 2 inteiro do 0031, com o bloco de máquina) e a medida do custo de N enlaces ociosos, que o 0031 admite não ter.

### Perguntas ao dono

- Qual caso de uso motiva a «conexão simultânea»: (i) ler e escrever em B enquanto fala em A, (ii) só saber que algo aconteceu em B, ou (iii) hospedar o próprio servidor e visitar outro sem derrubá-lo? (i) é a alternativa (a), GG; (ii) é a (b), G; (iii) é a (d), M.
- A 1.0 pode sair anunciando «seu servidor continua no ar enquanto você visita outro», com a multiconexão de verdade na 1.1?
- Ao trazer outro servidor para a frente estando numa sala de voz: a voz sai junto (mais barato, alternativa b) ou fica na sala antiga, como o ADR 0031 decide (alternativa a)?
- MODs de um servidor em segundo plano: você aceita que não rodam (tema, MESA, PERFIS só na frente) e que voltar para a frente os monta de novo?
- Estar «presente» no roster de B enquanto olha A é aceitável? O protocolo não tem estado de ausente, e cada conexão quente aparece como presente para todos.
- Qual o teto de sessões simultâneas? O 0031 registra que não há número nem medida, e cada sessão custa thread, socket, enlace e um laço de estado.
- O ADR 0031 passa de «proposto» a aceito, com a sequência (b) antes de (a), ou deve ser reescrito à luz da geração E2 e da API 5 de MODs, que não o consideraram?

---

## Calls privadas

**Resumo do analista.** No HEAD e2fac4d (= v0.15.0 publicada, protocolo 8) não existe chamada privada em nenhuma das três leituras. Não há verbo de ligar, mensagem direta, sussurro nem sala temporária. Das peças de privacidade de sala que existem, nenhuma chega a quem usa: a senha por sala é conferida pelo servidor, mas nada fora dos testes consegue defini-la; o papel mínimo e a `ViewVoiceRoom` nunca são lidos; `MovePerson` passa por cima da senha e do teto; e a ocupação e o «falando» vão para o servidor inteiro sem filtro. O código favorece a leitura (1), uma chamada 1:1 dentro do servidor: tarefa de sala criada sob demanda, `assentar` com teto, presença e recusa de sala inexistente já existem, e faltam lista de membros, toque, filtro de difusão e casca. A leitura (2), ligar para quem não está no seu servidor, esbarra em vaga única de hospedagem, porta fixa, portaria ligada, `AlreadyConnected`, ausência de deep link e ausência de qualquer canal que avise o convidado. Os bytes de voz chegam em claro ao servidor, porque o TLS/QUIC termina nele. Por isso, hoje, «privada» só pode querer dizer «ninguém mais no servidor», nunca «nem quem hospeda». Nada no README nem na interface diz isso, e a `specs/01:55` afirma o contrário. Recomendação para a 1.0: «LIGAR» dentro do servidor, privada em relação aos outros membros, dita honestamente, com o protocolo já com lugar para E2EE. O E2EE 1:1 fica para a 1.1: custa pouco no servidor (nada), e o que pesa é criptografia no cliente e revisão.

### O que existe

- `construido_mas_nao_ligado`: Senha por sala de voz (servidor confere, casca pergunta) (seele-server/src/admissao.rs:303-351; session.rs:1938-1970; ui/tela-sessao.js:3344-3349 — setter só em testes (seele-conformance/tests/senha_da_sala.rs:106))
- `construido_e_publicado`: Teto por sala (limit) conferido na entrada (seele-proto/src/control.rs:1113-1121; seele-server/src/session.rs:1977-2000)
- `so_desenho`: Papel mínimo por sala de voz (minimum_role) (seele-server/src/persistence/schema.rs:87 — nunca lido)
- `so_desenho`: Permissão ViewVoiceRoom / lista de salas por pessoa (seele-proto/src/control.rs:390; seele-server/src/session.rs:1234-1243 manda todas)
- `construido_mas_nao_ligado`: Papel mínimo de leitura/escrita de canal de texto (seele-server/src/autorizacao.rs:104-135 (lê); :43-45 (nada escreve))
- `construido_e_publicado`: Mover pessoa para uma sala (operador) (seele-server/src/session.rs:2529-2538, 3387-3441, 4127-4150)
- `construido_mas_nao_ligado`: Aviso 'VOCÊ FOI CHAMADO' (AlertReason::Mentioned) (seele-proto/src/control.rs:714; apps/seele-app/ui/frases.js:121 — nunca emitido pelo servidor)
- `construido_e_publicado`: Presença no servidor (quem está conectado) (seele-proto/src/control.rs:2053-2068)
- `construido_e_publicado`: Tarefa de sala criada sob demanda para qualquer VoiceRoomId (seele-server/src/voice_room.rs:1238 (VoiceRooms::of))
- `ausente`: Chamada 1:1 / ligar / tocar / atender (nenhum arquivo (grep))
- `ausente`: Mensagem direta, sussurro, sala temporária (nenhum arquivo (grep))
- `construido_e_publicado`: Encaminhamento de voz sem tocar no payload (pré-requisito de E2EE) (seele-server/src/voice_room.rs:1051-1122; seele-proto/src/media.rs:86-92)
- `construido_e_publicado`: Identidade Ed25519 durável, uma por máquina (seele-core/src/identity.rs:16, 36-58)
- `parcial`: Primitivas X25519/HKDF/ChaCha20-Poly1305 (ring 0.17.14, via rustls) (Cargo.lock; seele-core/Cargo.toml:38 — não usadas para mídia)
- `so_desenho`: E2EE de mídia (specs/08-seguranca.md:55-65 (esboço); specs/09-roadmap.md:109 (pós-v1))
- `parcial`: Caminho entre pares (par.rs) (seele-core/src/par.rs:1 — carrega só tela, não voz)
- `construido_e_publicado`: Hospedagem embutida + convite de uso único (apps/seele-app/src/main.rs:1505-1630; seele-server/src/hospedagem.rs:322)
- `construido_e_publicado`: Portaria semeada ligada no HOSPEDAR AQUI (apps/seele-app/src/main.rs:1582, 1606; seele-server/src/portaria.rs:159-165)
- `construido_e_publicado`: Faixa de aviso fora das telas que não rouba foco (molde para o toque) (apps/seele-app/ui/camada-portaria.js:507 (avisarQueBatem))
- `ausente`: seele:// registrado como protocolo (deep link) (apps/seele-app/Cargo.toml:51,58 (só dialog/updater); Info.plist sem CFBundleURLTypes)
- `ausente`: Notificação do sistema (tocar com a janela minimizada) (apps/seele-app/ui/camada-portaria.js:456-458 (pendência 23))

### Lacunas

- **[BLOQUEANTE · esforço P · risco baixo] Nada conta a quem usa que quem hospeda recebe a voz em claro.** O QUIC termina no servidor, e o seeled recebe o payload Opus em claro e o encaminha intacto. Ele não decodifica, mas um binário alterado grava. A specs/08 manda que isso esteja escrito; a specs/01:55 afirma o contrário; README e interface não dizem nada. Uma 1.0 que chame qualquer coisa de «privada» sem dizer isso é uma promessa falsa de privacidade, exatamente o «o produto sabe e não conta». Consertar é texto: README, ajuda, specs/01:55 e o rótulo da chamada.
  - *Evidência:* seele-server/src/voice_room.rs:96, 1111-1122; specs/08-seguranca.md:17,65; specs/01-arquitetura.md:55; README.md:104-105; grep por 'em claro|pode ouvir|capturar' em README/ui sem resultado (MEDIDO)
- **[importante · esforço G · risco medio] Não existe chamada: ligar, tocar, atender, recusar, sala efêmera.** Nenhum verbo, estado ou tela de chamada. Criar sala exige ManageVoiceRooms, que o papel Pessoa não tem, então ninguém comum consegue abrir uma sala a dois. O servidor tem presença, tarefa de sala sob demanda e assentar com teto. Falta a camada de chamada por cima.
  - *Evidência:* grep sem resultado no proto e no servidor (MEDIDO); session.rs:2211; schema.rs:154; voice_room.rs:1238; session.rs:3899
- **[importante · esforço M · risco medio] Sala sem lista de membros e com ocupação e fala difundidas a todos.** ViewVoiceRoom nunca é conferida, minimum_role nunca é lido, todas as salas vão a todos, e PersonJoined, PersonLeft e PersonState (speaking) saem sem filtro para o servidor inteiro. Mesmo com uma sala a dois, qualquer membro vê que A e B estão juntos e quando cada um fala.
  - *Evidência:* session.rs:1234-1243, 4842-4856, 4888; server.rs:989, 1179; schema.rs:87 (MEDIDO: minimum_role só aparece ali)
- **[importante · esforço P · risco baixo] MovePerson passa por cima da senha e do teto da sala.** moderavel aceita quem == alvo, e o movimento senta com teto None, sem conferir a senha. Um Operador (papel padrão com MovePerson) entra em qualquer sala trancada. Hoje não pega ninguém, porque nenhuma sala tem senha. Vira furo no dia em que existir sala ou chamada privada.
  - *Evidência:* session.rs:2529-2538, 3424, 3905-3911, 4136-4138; schema.rs:153
- **[importante · esforço M · risco baixo] Toque de chamada não chegaria a quem está com a janela minimizada; o aviso 'VOCÊ FOI CHAMADO' nunca é emitido.** Não há tauri-plugin-notification (pendência 23, item 1), e AlertReason::Mentioned existe com frase na casca mas o servidor nunca o manda. Uma chamada tocaria só dentro da janela. Sem prazo e sem um estado NÃO ATENDEU, quem liga ficaria esperando calado.
  - *Evidência:* ui/camada-portaria.js:456-458; control.rs:714; ui/frases.js:121; grep -rln Mentioned (MEDIDO)
- **[importante · esforço P · risco alto] Cada verbo novo derruba a conversa entre 0.15 e 1.0; chamada e lugar para E2EE precisam caber numa subida só.** Todo quadro sai carimbado com PROTOCOL_VERSION global, e o par mais velho recusa. Chamada na 1.0 é protocolo 9. Se o E2EE vier depois pedindo campo novo, é outra subida e outra quebra. O campo de material de chave tem de nascer já na 1.0, opcional e sem uso.
  - *Evidência:* seele-proto/src/version.rs:124, 128-160; session.rs:4152 (entende_a_mensagem)
- **[desejável · esforço M · risco baixo] Senha de sala construída sem caminho para ser definida.** O servidor confere e a casca pergunta, mas nenhum verbo, subcomando do seeled ou tela grava a senha. É uma fechadura que ninguém consegue trancar. Ou ganha um verbo SetVoiceRoomPassword com ManageVoiceRooms, ou o prompt sai.
  - *Evidência:* admissao.rs:340; seele-server/src/main.rs:34-37; conformance/tests/senha_da_sala.rs:106 (MEDIDO: únicos chamadores)
- **[desejável · esforço GG · risco medio] Ligar para quem não está no seu servidor (ADR 0047 §4) não é produto.** A hospedagem é vaga única (JaHospedando), a porta é 8383 fixa, a portaria nasce ligada e o ADR 0030 não aprova sozinho quem traz convite. AlreadyConnected impede estar no servidor do grupo ao mesmo tempo. Não há deep link, e não há canal que avise o convidado: o ponto de encontro não repassa conteúdo e o quarto é por marca de servidor. Depende da multiconexão (ADR 0031), de uma revisão do ADR 0030 e do tauri-plugin-deep-link.
  - *Evidência:* main.rs:145, 899, 1537-1545, 1561, 1582, 1963; portaria.rs:61-64; seele-proto/src/encontro.rs:81-85; Cargo.toml:51,58
- **[desejável · esforço G · risco medio] E2EE de voz na chamada 1:1.** Sem ele, «privada» nunca significa «nem quem hospeda». O custo no servidor é zero, porque ele só lê ssrc e seq. No cliente: X25519 efêmero assinado pela identidade Ed25519, HKDF com duas chaves (uma por sentido), ChaCha20-Poly1305 com o cabeçalho como AAD, nonce explícito (+24 B por quadro), número de segurança e contagem visível de falhas de decifração. O roadmap põe E2EE em pós-v1; só vira 1.0 se o dono definir privada como «nem o dono».
  - *Evidência:* voice_room.rs:1057-1098; voice.rs:1914, 2080-2088, 1245-1256; identity.rs:36-58; control.rs:453-460; Cargo.lock (ring 0.17.14); specs/09-roadmap.md:109
- **[desejável · esforço G · risco medio] Mensagem direta de texto.** Só existem canais. Quem pode ler é decidido por papel, não por pessoa, e mesmo isso nunca é escrito. DM exige lista de membros por canal e uma forma de o canal existir só para dois.
  - *Evidência:* control.rs:1071-1081 (SendMessage por channel); autorizacao.rs:43-45

### O que o revisor conferiu

- **CONFIRMADA**: 1. Quem hospeda recebe a voz de qualquer sala em claro: o servidor encaminha o payload Opus intacto depois que o QUIC termina nele. Por isso, sem E2EE, «privada» só pode significar «ninguém mais do servidor».
  - LIDO: crates/seele-server/src/voice_room.rs:1057 decodifica só o cabeçalho (`(header, _payload)`); :1111-1122 faz `subscriber.outbound.try_send(bytes.to_vec())` com os bytes recebidos. crates/seele-core/src/voice.rs:2078-2086 monta `MediaHeader` + `encode_datagram(&payload)` e chama `media.send` sem cifra. MEDIDO: `grep -n 'encrypt|seal|cifr|chacha|aead'` em crates/seele-core/src/*.rs só acha comentários de identity.rs:17,20. MEDIDO: `grep -n 'MediaHeader|opus|ssrc|áudio|voz'` em crates/seele-core/src/par.rs só acha um comentário (:872), então não há caminho de voz de cliente a cliente. Na hospedagem embutida, o «servidor» é o próprio processo do app de quem hospeda (apps/seele-app/src/main.rs:1560 `Hospedagem::iniciar`). specs/08-seguranca.md:17 e :65 confirmam que isso está fora do escopo da v1. **Correção:** O fato está correto. A conclusão vale só para a leitura (1). No desenho do ADR 0047 §4.3 (docs/adr/0047-o-link-que-volta-a-funcionar-amanha.md:325-331), quem liga é quem hospeda. Nesse caso quem hospeda é um dos dois participantes, e «privada» pode significar «só nós dois» mesmo sem E2EE.
- **PARCIAL**: 2. O servidor já tem quase tudo para uma chamada 1:1 interna. Faltam lista de membros por sala, verbo de toque, filtro de difusão e fim da sala efêmera. Áudio e encaminhamento não mudam.
  - LIDO, e confirma o núcleo: VoiceRooms::of (voice_room.rs:1238-1245) cria a tarefa para qualquer id sem consultar o banco. `assentar` (session.rs:3899-3981) não depende de linha no banco. No cliente, state.rs:637-650 (`enter_voice_room`) e :1072-1089 (`PersonJoined`) aceitam qualquer VoiceRoomId. Nenhum dos critérios de refutação se cumpre. LIDO, e o que a lista de faltas deixa de fora: (i) o único caminho de cliente até `assentar` com teto é o EnterVoiceRoom, que exige a linha em `voice_rooms` duas vezes: `voice_room_liberado` (session.rs:1963-1974, admissao.rs:316-319) e a leitura do teto (session.rs:1981-1998, `let Some(teto) else VoiceRoomEntryRefused`). Se a sala for gravada no banco, ela vaza, porque `read_server` (session.rs:1234-1243) manda todas as linhas no aperto de mão e `ViewVoiceRoom` nunca é lido. (ii) O mapa de tarefas nunca encolhe: MEDIDO, `grep self.tasks` em voice_room.rs só acha entry/values/get/len (:1215,1229,1239,1260,1269) e nenhum remove. Assim, cada chamada deixa uma tarefa viva até o reinício, e `leave_everywhere` (:1259-1265) e a `Subida` (:1215) varrem todas as tarefas já criadas. O comentário em :1253-1254 dimensiona isso em «five sends». (iii) `assentar` chama `leave_everywhere(person, None)` e `encerrar_telas_da_pessoa` (session.rs:3923,3929): atender tira a pessoa da sala do grupo e derruba a tela que ela compartilha. (iv) O único verbo de «puxar alguém para uma sala» que existe é o MovePerson, e ele não pede consentimento. (v) O filtro de difusão contraria uma premissa escrita: session.rs:4830-4839 justifica mandar `PersonJoined` a todos porque isso «reveals nothing that walking into the room would not», e essa frase deixa de ser verdade para uma sala privada. **Correção:** O núcleo de mídia de fato não muda. Faltam também: um caminho de entrada que não dependa da linha no banco (ou uma linha com visibilidade filtrada, o que exige ler ViewVoiceRoom/minimum_role), a limpeza das tarefas no mapa de VoiceRooms, o consentimento e a decisão de produto sobre sair da sala do grupo ao atender. O custo fica acima de «quase tudo pronto».
- **CONFIRMADA**: 3. As fechaduras de sala que existem não funcionam para quem usa: a senha não tem caminho de produção para ser definida, e o MovePerson passa por cima dela e do teto.
  - MEDIDO: `grep -rn 'definir_senha_voice_room|password_hash' --include='*.rs'` acha só a definição (admissao.rs:340-351), testes (seele-conformance/tests/senha_da_sala.rs:106, acceptance_seguranca.rs:213, admissao.rs:684,765), a leitura (admissao.rs:309), o schema e channels.rs:66,149. LIDO: channels.rs:145-149 diz que nada define senha na criação. O `seeled senha` (crates/seele-server/src/main.rs:36, 220-236) chama `admissao::definir_senha`, que é a senha do servidor inteiro, não a de uma sala. Na lista de variantes de ClientMessage (control.rs:1025-1571) não há verbo de senha de sala. MovePerson: o handler (session.rs:2529-2538) só confere `moderavel`. O braço `Event::PersonMoved` (:3387-3441) chama `assentar(..., None)` em :3424 sem `voice_room_liberado`. `moderavel` devolve true quando quem == alvo (:4136-4138). **Correção:** Duas ressalvas. O MovePerson só vem por padrão com Commander e Operator (persistence/schema.rs:152-153). Passar por cima do teto é decisão documentada (session.rs:3906-3915), não descuido. Então «passa por cima» é um privilégio de moderador, não uma brecha para qualquer membro. Há ainda algo que o analista não viu: o handler do MovePerson também não confere se o `destino` existe (ver omissões).
- **CONFIRMADA**: 4. Fora de um servidor comum não há como avisar alguém de uma ligação: o ponto de encontro não repassa conteúdo, o seele:// não abre o app e não há notificação do sistema.
  - LIDO: crates/seele-proto/src/encontro.rs:81-85 diz que a resposta é montada campo a campo e que nenhum byte escolhido por quem manda chega a quem recebe. Os únicos plugins do app são tauri-plugin-dialog e tauri-plugin-updater (apps/seele-app/Cargo.toml:51,58). As únicas chaves em apps/seele-app/Info.plist são NSLocalNetworkUsageDescription, NSScreenCaptureUsageDescription e NSMicrophoneUsageDescription (:84-88). tauri.conf.json:56-66 só configura o updater. tauri.linux.conf.json não tem MimeType. O modelo NSIS (instalador.nsi:747-752) tem o laço `deep_link_protocols`, mas ele sai vazio porque nada está configurado. camada-portaria.js:456-458 atribui a falta de notificação à pendência 23. **Correção:** Duas ressalvas que reduzem o custo. (a) O app já abre um link recebido por argumento: `--entrar <link>` (main.rs:260-288) é usado pelo launcher e testado em versoes.rs:478-497. O que falta é só o registro do esquema no sistema operacional. (b) Na leitura (2) do ADR 0047, o aviso é o próprio link mandado por fora (WhatsApp etc.), como a hospedagem já funciona hoje. A ausência de canal só bloqueia um «toque» automático, não uma chamada.
- **CONFIRMADA**: 5. E2EE da voz numa chamada 1:1 não exige mudar o encaminhamento: o servidor só lê ssrc e seq do cabeçalho, e o «falando» vem da chegada do datagrama, não do conteúdo.
  - LIDO: voice_room.rs:1057 (decode), :1062 (ssrc) e :1085 (`member.perda.chegou(header.seq, now)`). MEDIDO: `grep 'header.timestamp|\.timestamp'` em crates/seele-server/src não acha nada. O «falando» vem de `chegou`/`falando` (session.rs:150-168), que só olham a hora do último datagrama. MAX_PAYLOAD_LEN = 1275 (seele-proto/src/media.rs:43) e MAX_BITRATE_BPS = 48_000 (seele-proto/src/transport.rs:84) dão cerca de 120 B por quadro de 20 ms, e +16 B de tag mais 8 B de nonce cabem com folga. O servidor também lê o byte de versão, com igualdade estrita (media.rs:184-188), mas isso não impede E2EE. **Correção:** A afirmação vale para voz. Duas ressalvas de desenho: (a) o caminho da tela lê o byte de tipo de quadro para decidir a porta de entrada no quadro-chave (seele-server/src/tela.rs:925-940), então um E2EE de tela teria de deixar esse byte em claro; (b) `seq` é u16 e dá a volta em cerca de 22 min a 50 pacotes/s (media.rs:98-100), então o nonce não pode derivar só de ssrc+seq.
- **PARCIAL**: 6. Qualquer verbo novo de chamada quebra a conversa entre 0.15 e 1.0, então o lugar do E2EE tem de nascer no mesmo protocolo da chamada para não exigir uma segunda quebra.
  - LIDO, e confirma o mecanismo: control.rs:2301 `frame.push(PROTOCOL_VERSION)` carimba o número global; version.rs:221-235 `negotiate` recusa quem vier com versão maior (PeerTooNew); media.rs:184-188 exige igualdade estrita no datagrama. Uma variante nova exige subir a versão, e subir quebra as duas direções. LIDO, e contradiz a premissa «qualquer chamada»: docs/adr/0047-o-link-que-volta-a-funcionar-amanha.md:325-331 propõe a chamada como hospedagem embutida com uma sala só e convite de uso único, e diz «Nenhum verbo novo de protocolo». LIDO, e atenua o custo da quebra: o launcher do ADR 0046 está em parte construído (apps/seele-app/src/versoes.rs; main.rs:6926-6933, `pode_abrir_naquela_versao`). O link carrega a versão do servidor e abre a versão correspondente quando ela está instalada. **Correção:** O mecanismo está certo. A conclusão é inferência. Uma chamada no desenho do ADR 0047 não precisa de verbo novo. Quanto à «segunda quebra», o projeto sobe o protocolo quase a cada versão menor (de 7 para 8 na 0.15), então ela só pesa se a 1.0 prometer estabilidade de protocolo. Reservar agora o lugar do E2EE é sensato, porque o cabeçalho de mídia não tem bit de flag (media.rs:6-9), mas é escolha de produto, não obrigação técnica.

### Severidades contestadas

- **Nada conta a quem usa que quem hospeda recebe a voz em claro**: o revisor propõe *importante (vira bloqueante_1_0 só se a 1.0 lançar algo rotulado «privado» na leitura 1, isto é, dentro de um servidor hospedado por um terceiro)*. O fato está certo, e a omissão tem agravantes que o analista não citou. O README diz «O TLS é ponta a ponta» (README.md:105), e um leigo lê isso como E2E. A interface mostra «CONEXÃO SEGURA» (apps/seele-app/ui/index.html:3864-3865). Mesmo assim, o dano grave depende de três condições: um recurso chamado «privado», um anfitrião que não é participante e um binário alterado. O modelo declarado é «você hospeda para o seu próprio grupo» (specs/08-seguranca.md:65), a mesma postura de Discord, TeamSpeak e Mumble, e o spec já tira o E2EE da v1 (:17). Na leitura (2), que é o que o dono pediu em 15/09 («conexão P2P sem server para conversa privada», ADR 0047:17-20) e que o ADR 0047 §4.3 desenha, quem hospeda é quem liga, e o risco desaparece. O conserto é um parágrafo no README e uma linha na ajuda, então deve entrar antes da 1.0 de qualquer jeito, mas é classe «importante». A specs/01:55 é interna e não prejudica usuário.

### O que o analista não viu

- O pedido original do dono favorece a leitura (2), não a (1) que o analista recomenda. O ADR 0047 registra o pedido nas palavras dele: «(b) conexão P2P «sem server» para conversa privada» (docs/adr/0047-o-link-que-volta-a-funcionar-amanha.md:17-20). O §4.3 (:325-351) já tem um desenho para isso: a chamada é a hospedagem embutida de quem liga, com uma sala e convite de uso único, sem verbo novo de protocolo, com esforço «pequeno a médio», e o ADR a chama de melhor relação valor/risco. Nesse desenho, quem hospeda é um dos participantes, então a questão de «a voz em claro para quem hospeda» não se aplica. O analista lista os mesmos obstáculos que o ADR (vaga única em main.rs:145, porta fixa em main.rs:1963, portaria em main.rs:1582, AlreadyConnected em main.rs:899), mas os apresenta como impeditivos, quando o ADR os trata como decisões de produto. LIDO.
- A portaria atrapalha a leitura (2) mais do que parece: um convite válido NÃO aprova sozinho quando a portaria está ligada (crates/seele-server/src/portaria.rs:65 e o teste em :666-677, «E não aprova sozinho. O convite é prova exibida, não decisão»). Uma chamada sobre HOSPEDAR AQUI obriga quem liga a aprovar manualmente a pessoa para quem acabou de ligar, a menos que o servidor da chamada suba sem `semear_ligada`. LIDO.
- O MovePerson não confere se a sala de destino existe. O handler (session.rs:2529-2538) só confere `moderavel`. O braço PersonMoved chama `assentar(..., None)` (:3424), e VoiceRooms::of cria a tarefa para qualquer id (voice_room.rs:1238-1245). Então um cliente modificado com papel Operator (schema.rs:153) pode sentar qualquer pessoa, inclusive a si mesmo por quem==alvo (:4136-4138), numa sala-fantasma que não está no banco. O `PersonJoined` com o id desconhecido vai para todos (session.rs:4842-4850), e a pessoa some de todos os rosters, porque state.rs:850 (`roster(voice_room)`) é por sala listada e a casca só oferece salas da lista (camada-moderar.js:847). O resultado é uma sala efêmera que já existe na prática, sem limpeza e sem aviso: «o produto sabe e não conta». LIDO; não reproduzido.
- O mapa de tarefas de sala nunca encolhe. MEDIDO: `grep self.tasks` em voice_room.rs não acha nenhum remove, e nem o DeleteVoiceRoom limpa. Qualquer desenho de sala efêmera por chamada (leitura 1) acumula tarefas e aumenta o fan-out de `leave_everywhere` (:1259-1265) e da `Subida` (:1215) até o reinício. O comentário de :1253-1254 supõe cinco salas. Isso é pré-requisito concreto de «fim da sala efêmera», que o analista cita sem apontar onde mora.
- Presença «Não perturbe» existe no protocolo e no servidor, mas não está ligada: `Presence::DoNotDisturb`/`Away` (control.rs:378-382), `ClientMessage::SetPresence` tratado em session.rs:2182 e `Client::set_presence` em seele-core/src/client.rs:939. MEDIDO: nenhuma chamada em crates/seele-ffi, apps/seele-app/src/main.rs ou ui/*.js. Um «toque» de chamada precisaria respeitar o DND, e a peça está construída mas não ligada.
- Atender uma chamada na leitura (1) tira a pessoa da sala do grupo e encerra a tela que ela compartilha: `assentar` chama `leave_everywhere(person, None)` e `encerrar_telas_da_pessoa` (session.rs:3923,3929). É uma decisão de produto que o analista não coloca, e ela muda a UX de «ligar» dentro de um servidor. LIDO.
- O próprio código registra a premissa que a chamada privada quebra: session.rs:4830-4839 justifica difundir ocupação e PersonState a todos porque «reveals nothing that walking into the room would not». O «filtro de difusão» não é só código novo: é reverter uma decisão escrita, e os testes que dependem dela precisam ser revistos. LIDO.
- A base de qualquer «chamada» está sem verificação de campo recente: as notas da 0.14 dizem que «nenhuma chamada entre duas máquinas nativas foi feita nesta volta» (docs/notas-da-v0.14.0.md:6,317), e o README (último commit aa77ad8, 31/08) ainda diz que voz por microfone real entre duas máquinas não foi verificada (README.md:288-290). Construir «LIGAR» sobre voz sem validação entre máquinas repete o «existir não é funcionar». LIDO; o estado real nas máquinas não foi verificado.

### Desenho proposto pelo analista

## Recorte da 1.0: «LIGAR» dentro do servidor, privada em relação aos outros membros e dita com honestidade

Das três leituras, a (1b), chamada 1:1 dentro do servidor, é a que o código já sustenta quase inteira e a única que resolve o toque: o chamado já está com a conexão de controle aberta. A (2) fica fora da 1.0: depende da multiconexão e não tem por onde tocar. A (3) vai para a 1.1, com o lugar dela reservado já no protocolo da 1.0.

### E0 · Honestidade (horas; condição para qualquer coisa chamada «privada»)
- `specs/01-arquitetura.md:55`: trocar por «o servidor não decodifica o áudio, mas o recebe em claro».
- README e ajuda: uma frase dizendo que quem hospeda encaminha a voz e tecnicamente poderia gravá-la, e que a cifra é entre você e o servidor. `README.md:104-105` passa a dizer «entre você e o servidor».
- O rótulo da chamada: «CHAMADA PRIVADA NESTE SERVIDOR · só vocês dois ouvem · quem hospeda encaminha o áudio».

### E1 · Protocolo: uma subida só, 8 → 9 (cerca de 1 dia)
Variantes **no fim** das duas listas, como o `control.rs` exige, e caladas para quem não as entende em `session::entende_a_mensagem` (`session.rs:4152`):
- `ClientMessage::Ligar { para: PersonId, material: Option<Vec<u8>> }`
- `ClientMessage::Atender { chamada: VoiceRoomId, material: Option<Vec<u8>> }`
- `ClientMessage::Recusar { chamada }`
- `ServerMessage::Chamando { chamada, de: PersonProfile, material }`
- `ServerMessage::Chamada { chamada, estado: Tocando|Atendida|Recusada|NaoAtendeu|Ocupado|Encerrada, material }`

`material` fica limitado por `check_bounds` a 256 bytes e **não é usado na 1.0**. É o assento do E2EE: a 1.1 liga a cifra sem nova quebra de protocolo, e cada quebra custa derrubar a versão anterior inteira (`version.rs:128-160`). Desligar é o `LeaveVoiceRoom` que já existe. Se outras frentes da 1.0 precisarem de protocolo, tudo entra nesta mesma subida.

### E2 · Servidor (3 a 4 dias), módulo novo `seele-server/src/chamadas.rs`
- **Tabela em memória**, `VoiceRoomId → Chamada { membros: [PersonId; 2], estado, tocou_em }`, com ids numa faixa alta reservada (≥ 2^31) que **nunca vai ao banco**. Isso dispensa migração, e dá uma defesa de graça: um `EnterVoiceRoom` de terceiro com o id da chamada cai em `voice_room_liberado`, que recusa sala inexistente no banco (`admissao.rs:316-319`).
- **`Ligar`** exige `EnterVoiceRoom` e `Speak`, **não** `ManageVoiceRooms`. O alvo precisa estar em `server.presentes`, ser outra pessoa e estar livre (senão `Ocupado`). Uma chamada tocando por vez por quem liga. Quem liga é sentado com `assentar(..., Some(2))` (`session.rs:3899`), que já tira a pessoa da sala anterior: um caminho de voz só, dentro do orçamento do ADR 0009. `Chamando` vai a **todas as sessões** do chamado. Com 30 s sem resposta, `NaoAtendeu` e a sala acaba.
- **`Atender`** confere se quem atende é membro e o senta com teto 2. `Recusar` e `NaoAtendeu` avisam quem ligou. **Nunca calado.**
- **`MovePerson`** para um id de chamada é recusado no braço de `session.rs:2529`.
- **Filtros em `translate`** (`session.rs:4709`): `PersonJoined`/`PersonLeft` de sala de chamada só para os membros (`:4842-4856`); `Occupancy::everywhere` (`server.rs:1179`) sem as chamadas; `PersonState.speaking` de quem está em chamada vai como `false` para quem não é membro (`:4888`).
- **Fim**: a sala sai da tabela quando esvazia. A retomada de assento na reconexão (`session.rs:3905-3911`) precisa achar a chamada viva durante a janela de queda silenciosa (15 a 20 s, `voice_room.rs:69-76`).
- **Encaminhamento: nada muda.** A tarefa por sala (`voice_room.rs:1238`) já serve.

### E3 · Core e FFI (cerca de 2 dias)
- `Room`, em `seele-core/src/state.rs`, ganha `toque: Option<{chamada, de}>` e `chamada: Option<{sala, com, estado}>`. A sala de chamada é sintetizada no cliente, porque não vem na lista de salas.
- O snapshot expõe os dois.
- Comandos Tauri: `ligar`, `atender`, `recusar`.

### E4 · Casca (cerca de 3 dias)
- Botão **LIGAR** no cartão de cada pessoa presente.
- Faixa de toque **fora das telas**, no molde de `avisarQueBatem` (`ui/camada-portaria.js:507`), que **não chama `focus()`**, por causa do push-to-talk. A faixa leva ATENDER e RECUSAR.
- `ui/tela-chamada.js` sem mudança: a grade de cartões já desenha dois.
- Frases para NÃO ATENDEU, RECUSOU, OCUPADO e CAIU.
- Opcional na 1.0, com +1 dependência: `tauri-plugin-notification`, para tocar com a janela minimizada (pendência 23, item 1).

### E5 · Provas (cerca de 2 dias). Existir não é funcionar.
Testes em `seele-conformance` contra servidor real:
1. Terceiro com o id da chamada não entra.
2. `MovePerson` para a chamada é recusado.
3. Terceiro não recebe `PersonJoined` nem `speaking` da chamada.
4. 30 s sem resposta viram `NaoAtendeu`.
5. Queda e volta de um membro re-senta na chamada.

Cada teste é provado **revertendo o guarda e vendo-o falhar**. A CI só roda por `workflow_dispatch`: rodar à mão antes do release e anotar a saída.

**Total: G, cerca de 2 semanas. Nada muda no áudio do cliente nem no encaminhamento do servidor.**

**Limite a dizer na tela:** só se liga para quem está conectado **a este servidor agora**. Não há presença global, e a `AlreadyConnected` (`main.rs:899`) impede estar em dois servidores. A multiconexão do ADR 0031 aumenta o alcance da chamada, mas não é requisito dela.

### Alternativa mais barata, se nem isso couber
Um verbo `SetVoiceRoomPassword` com `ManageVoiceRooms`, um campo no lápis da sala, e o `MovePerson` passando a respeitar senha e teto. Leva uns 3 dias e custa a mesma quebra de protocolo. **Não recomendo como resposta a «calls privadas»:** a senha é compartilhada, a sala e os ocupantes continuam visíveis para todos, e só quem administra consegue trancar.

## 1.1 · E2EE na chamada 1:1 (G, sobre o recorte acima)
- Cada ponta gera uma X25519 efêmera (`ring::agreement`, já no `Cargo.lock`) e a assina, junto com o id da chamada, com a identidade Ed25519 (`identity.rs`). Vai em `material`, no `Ligar` e no `Atender`. O servidor acrescenta a chave pública Ed25519 de quem liga no `Chamando`; ele a conhece, `portaria.rs:92-93`.
- Contra o servidor trocar chaves: **número de segurança** = SHA-256 das duas chaves de identidade, lido em voz alta na própria chamada, mais um pin por chave de pessoa no molde do `tofu.rs`.
- HKDF-SHA256 gera **duas chaves, uma por sentido**. ChaCha20-Poly1305 usa os 11 bytes do cabeçalho como AAD.
- **Nonce explícito de 8 bytes**: época aleatória de 4 bytes por abertura do caminho, mais contador de 4. O motivo são os dois caminhos vivos na troca de aparelho (`voice.rs:1245-1256`). Custa +24 B por quadro, cerca de +9,6 kbps por fluxo. O limite de 1275 bytes (`media.rs:43`) sobra.
- **Onde entra no código**: cifra entre `voice.rs:2067` e `:2088`; decifra depois de `voice.rs:1914`. A chave vai nos `Controls` e atravessa `carregar_controles` (`voice.rs:1243`).
- **No servidor: zero linhas.** Ele só lê ssrc e seq (`voice_room.rs:1057-1098`), e o «falando» vem da chegada (`session.rs:150-168`).
- Falha de decifração **contada e mostrada** («o áudio de X chegou e não abriu»), nunca silêncio.
- O cadeado diz «só a voz»: a tela passa por `tela.rs` em claro.
- A composição dos primitivos é nossa, e a `specs/08` proíbe criptografia caseira: precisa de revisão externa antes de ser chamada de E2EE.

**Por que não voz P2P via `par.rs` no lugar do E2EE:** o `par.rs` só carrega tela. Sem retransmissão (ADR 0022, degrau 5, fora de escopo), NAT simétrico não fura. E a impressão digital do par é anunciada pelo servidor (`control.rs:2153-2160`), que poderia se pôr no meio do mesmo jeito.

## Depois da 1.0 · ligar para quem não está no seu servidor (GG)
Pressupõe os passos 1 e 2 do ADR 0031 (evento com dono, voz fora da `Connection`). Depois:
- Uma segunda vaga de hospedagem, com porta diferente de 8383 e banco `Location::Memory` (`persistence/mod.rs:91`).
- Portaria **desligada nesse banco**, com convite de uso único do ADR 0021. É uma revisão explícita do ADR 0030 para este caso.
- `tauri-plugin-deep-link`, para o link mandado pelo WhatsApp abrir o app.

O «toque» é o próprio link. **Não há como tocar sem infraestrutura nova**: o ponto de encontro não repassa conteúdo por decisão (`seele-proto/src/encontro.rs:81-85`).

### Perguntas ao dono

- «Privada» quer dizer «ninguém mais do servidor ouve nem vê que a chamada existe» ou «nem quem hospeda o servidor»? A segunda leitura é E2EE: soma de 1 a 2 semanas, exige revisão de criptografia e o roadmap a põe em pós-v1.
- A chamada é só 1:1, ou precisa de «chamar mais um» (grupo pequeno privado)? A lista de membros e o teto mudam de forma.
- Ligar para alguém que não está conectado ao mesmo servidor é requisito da 1.0? Hoje não há canal nenhum para avisar essa pessoa; só um link mandado por fora e colado à mão.
- Um Operador ou Comandante pode puxar alguém para fora de uma chamada ou encerrá-la? Entrar nela, a proposta recusa.
- Aceita uma dependência nova, tauri-plugin-notification, para a chamada tocar com a janela minimizada? Sem ela, o toque só aparece com o app à vista.
- A 1.0 aceita mais uma quebra de protocolo (a 0.15 não conversa com a 1.0)? Se sim, dá para juntar nela todas as mudanças de protocolo da 1.0, incluindo o campo reservado para E2EE?
- Compartilhar tela dentro de uma chamada privada vale? Mesmo com E2EE de voz, a tela continua passando em claro pelo servidor.

---

## Estabilidade e qualidade

**Resumo do analista.** A base de testes é grande: 2.395 atributos de teste no HEAD. Mesmo assim, hoje nada prova automaticamente que o código publicado funciona. A CI não passa em nenhuma execução desde 16/08. A última, sobre o mesmo código da v0.15.0 (14d9c30 → e2fac4d só mexeu em workflows e docs), reprovou em 5 de 9 jobs, nos três sistemas. Desde 22/09 ela só roda quando alguém dispara pela aba Actions, e as versões são montadas na máquina do dono, por fora do portão de publicação que a revisão v15 criou. A função central, voz entre duas máquinas, nunca passou pelo roteiro de validação, e a v0.15 liga por padrão uma redução de ruído na força máxima que só foi calibrada com sinal sintético. O protocolo quebrou a compatibilidade 7 vezes desde 31/08, duas delas entre versões publicadas nos últimos 5 dias, e a saída que as notas da 0.15 oferecem (baixar a 0.14.2 e guardá-la ao lado) não existe no app. O mínimo para chamar de 1.0 tem quatro partes: (1) CI verde nos três sistemas, com a publicação amarrada a ela; (2) um contrato de protocolo congelado, com guarda e com tolerância a mensagem desconhecida, o que é viável porque cada quadro de controle leva o próprio comprimento; (3) uma sessão registrada Windows↔Windows e Windows↔Mac; (4) quatro consertos pequenos de 'o produto sabe e não conta'. Assinatura, notificação do sistema, seele:// clicável, virtualização do histórico e backup cabem na 1.0.x.

### O que existe

- `construido_mas_nao_ligado`: CI em três sistemas (fmt, regras, clippy e test) mais o job de bancadas de navegador (.github/workflows/ci.yml (gatilho só manual desde 6f970c9, linha 28))
- `construido_mas_nao_ligado`: Portão R07: empacotar só depois de validar verde nos três sistemas (.github/workflows/release.yml:133-205)
- `construido_e_publicado`: Publicação local com bateria (Mac, e Windows por SSH) e saída --sem-bateria (empacotar/publicar.sh:936-1045)
- `parcial`: Versões lado a lado (depósito, lançador, camada VERSÕES) (crates/seele-lancador; apps/seele-app/src/versoes.rs:321-367; ui/camada-versoes.js:111-119)
- `construido_mas_nao_ligado`: Manifesto com várias versões (schema 2) já lido pelo lançador (crates/seele-lancador/src/manifesto.rs:1-35 (o latest.json publicado ainda descreve uma versão só))
- `so_desenho`: Seletor de versão ou negociação real de protocolo (ADR 0046) (docs/adr/0046-toda-versao-continua-de-pe.md (proposto))
- `ausente`: Guarda da codificação do fio (vetores dourados de ClientMessage/ServerMessage) (não encontrado em crates/seele-proto (só existe tests/vetores_de_hash.rs))
- `construido_e_publicado`: Quadros de controle com o comprimento na frente (base para tolerar mensagem desconhecida) (crates/seele-core/src/frame.rs:25-43; crates/seele-server/src/frame.rs:83-131)
- `parcial`: Roteiro de teste entre duas máquinas (docs/teste-duas-maquinas.md (manda compilar --bin connection, que não existe))
- `construido_mas_nao_ligado`: Teste de rede real entre duas máquinas (crates/seele-conformance/tests/duas_maquinas.rs:46 (#[ignore]))
- `construido_e_publicado`: Supressão de ruído e limiar adaptativo (F01/F02) ligados por padrão (crates/seele-core/src/voice.rs:688; crates/seele-audio/src/supressao.rs; ADR 0055)
- `construido_e_publicado`: Relógio de reprodução que repõe quadros vencidos (pendência 15) (crates/seele-audio/src/playout.rs)
- `construido_e_publicado`: Troca de fone e microfone no meio da conversa (pendência 31) (seele_core::som_que_segue; testes de conformidade troca_de_aparelho.rs)
- `construido_mas_nao_ligado`: Contadores de perda na sala de voz e de atraso de sessão (crates/seele-server/src/voice_room.rs:421; server.rs:1210-1235 (lidos só em teste))
- `construido_e_publicado`: Log do app com rotação (apps/seele-app/src/main.rs:7690-7760)
- `ausente`: Exportar diagnóstico ou abrir o log pela interface (nenhum comando nem tela em apps/seele-app/ui)
- `ausente`: Prazo na abertura do aparelho de áudio (crates/seele-audio/src/device.rs:665 (sem prazo))
- `construido_e_publicado`: Atualização automática assinada (minisign, .sig e latest.json) (apps/seele-app/tauri.conf.json:56-65; assets da v0.15.0)
- `so_desenho`: Assinatura Apple e Microsoft (Azure Artifact Signing, notarização) (release.yml; docs/assinatura-e-atualizacao.md; ADR 0026 (sem credencial))
- `ausente`: Teste de fumaça do binário nativo por sistema (pendência 47) (—)
- `parcial`: Registro de pendências atualizado para a 0.15 (docs/pendencias.md (última mudança em 18/09, 2da00a2))

### Lacunas

- **[BLOQUEANTE · esforço M · risco baixo] A CI está vermelha nos três sistemas desde 16/08, e ninguém a dispara desde 22/09.** Nenhuma execução verde há cinco semanas. O código que virou a v0.15.0 reprovou em clippy (macOS e Windows) e em test (Linux, Windows e macOS). Não se sabe se as falhas são do produto, do ambiente ou de teste frágil, e é essa ignorância que bloqueia: lançar uma 1.0 com `test (windows)` vermelho de causa desconhecida é publicar às cegas na plataforma de quem relata os defeitos. Desde 6f970c9 o push não dispara nada, então uma regressão nova não aparece para ninguém.
  - *Evidência:* MEDIDO: API /actions/runs. Última verde é o run 31923099263 (16/08). Nas 100 execuções mais recentes, 0 verdes. Run 35762838943 (14d9c30): 5 de 9 jobs vermelhos. git diff 14d9c30..e2fac4d não toca nenhum .rs. ci.yml:28 só tem workflow_dispatch. Os logs dão 403 sem login.
- **[importante · esforço P · risco baixo] As versões saem da máquina do dono, por fora do portão R07.** O portão da revisão v15 (empacotar só depois de validar nos três sistemas) só existe no release.yml, e nenhuma versão recente passou por ele. publicar.sh tem bateria própria: o clippy dele não usa --all-features, o que diverge da CI, e o script aceita --sem-bateria. Não fica registro público de qual bateria rodou para cada versão. O comentário em ci.yml:17-20 afirma o contrário.
  - *Evidência:* MEDIDO: corpo do release v0.15.0 («Fora da integração contínua»). Os runs de Release de 5fe1768, 9755fea e 0018e71 falharam, e as versões 0.14.0, 0.14.1 e 0.14.2 foram publicadas mesmo assim. LIDO: publicar.sh:936-1045 e 888-891; release.yml:195-205.
- **[BLOQUEANTE · esforço M · risco medio] Cada subida de protocolo separa o grupo, e a saída que as notas da 0.15 oferecem não existe.** O protocolo subiu 7 vezes desde 31/08, duas delas entre versões publicadas em 5 dias (de 6 para 7 na 0.12.0, de 7 para 8 na 0.15.0). O quadro sai carimbado com a versão global e a janela é 1, então versões vizinhas não se falam em direção nenhuma. Numa 1.0, uma 1.0.1 que subir o protocolo tira do ar quem não atualizou junto. As notas da 0.15 prometem guardar a 0.14.2 ao lado, mas o app só baixa a versão mais nova do catálogo, e o catálogo publicado lista uma versão só. No Windows não há lado a lado. É o pior caso de 'o produto sabe e não conta': ele conta uma saída que não existe.
  - *Evidência:* MEDIDO: git show nos commits de release (0.11.2=6; 0.12.0 a 0.14.2=7; 0.15.0=8); o latest.json da v0.15.0 tem só version 0.15.0. LIDO: control.rs:2298-2330; version.rs:124,164; main.rs:5498-5531; versoes.rs:291-296 e 321-367; ui/camada-versoes.js:111-119; empacotar/notas/0.15.0.md:11-13.
- **[BLOQUEANTE · esforço M · risco baixo] Voz entre duas máquinas nunca validada por roteiro, e a v0.15 liga por padrão um filtro que nenhum ouvido testou.** A função central do produto não tem registro de uma validação ponta a ponta com microfone real (M1.15 e M1.16 vazios). A v0.15 passou a ligar por padrão a supressão de ruído na força máxima e o limiar que se ajusta à sala, os dois calibrados só com sinal sintético — e sinal sintético já escondeu 5 defeitos nesse mesmo caminho em 22/09. Se a supressão deformar a voz, a 1.0 piora a conversa de todo mundo por padrão. O roteiro que faria a validação está quebrado: manda compilar o `connection`, que não existe.
  - *Evidência:* LIDO: README.md:288; docs/m1-medicoes.md (sem M1.15/M1.16); duas_maquinas.rs:46 #[ignore]; voice.rs:688 SUPRESSAO_PADRAO=1_000; main.rs:6058; entrega-review-v15:93-100; auditoria-f01-f02 (A01 a A05); teste-duas-maquinas.md:20.
- **[importante · esforço M · risco medio] Abrir o aparelho de áudio não tem prazo, e trava o conectar inteiro.** Ao conectar, o app abre o microfone e a saída de áudio antes de dizer à tela que a conexão está pronta, e nesse caminho nada tem prazo: nem a abertura, nem a espera de quem chamou. Um bloqueio no CoreAudio deixa a tela de conexão parada para sempre, sem frase nenhuma, e ainda impede o keepalive da conexão. O bloqueio já foi visto duas vezes: na cópia QA e no teste de loopback no Mac do dono. O desvio 'sem aparelho, segue só com texto' já existe; falta o prazo que leva até ele.
  - *Evidência:* LIDO: seele-ffi/src/lib.rs:3938-3955 (abre o áudio antes de ready.send) e 1370-1372 (recv sem prazo); seele-audio/src/device.rs:665; jornadas-pendentes-2026-09-20 («A abertura do áudio não tem prazo… não foi mexido»); review-v15:308. A frequência no app é INFERIDA.
- **[importante · esforço P · risco baixo] A portaria deixa qualquer um entrar se o banco falhar ao ler a senha.** Politica::carregar transforma qualquer erro do SQLite em 'sem senha' e 'sem convite', e um servidor fechado passa a aceitar qualquer pessoa. O chamador foi escrito para recusar nesse caso, mas o erro nunca chega até ele. A probabilidade é baixa (erro de disco, banco corrompido, lock acima de 5 s), mas é uma falha que abre em vez de fechar, na porta de entrada, e se conserta em uma linha, como o módulo vizinho já faz.
  - *Evidência:* LIDO: seele-server/src/admissao.rs:85-101 (.ok() e .unwrap_or(0)) e 137-138 (Passe::livre); session.rs:506-510 (map_err que nunca dispara); o padrão certo está em portaria.rs:118-128 (.optional()?). Não reproduzido.
- **[importante · esforço M · risco baixo] Os contadores de perda só são lidos em teste, e ninguém consegue mandar um diagnóstico.** O servidor conta voz descartada por assinante atrasado, eventos perdidos e sessões encerradas por atraso, e nenhum desses números sai do processo. O `:sync` saiu junto com a TUI. O app grava um log com rotação, mas não oferece botão para abri-lo ou copiá-lo. Numa 1.0 com usuários de fora, todo relato como 'Windows ouve picotado' (pendência 15) chega sem dado nenhum.
  - *Evidência:* LIDO: voice_room.rs:421 e 1113-1128; server.rs:1210-1235; lidos só em par_lento.rs:281 e rajada.rs:159; main.rs:7690-7760; grep em apps/seele-app/ui sem nada que abra o log.
- **[importante · esforço P · risco baixo] O produto e a documentação mandam rodar um programa que não existe.** O `seeled` imprime, ao subir, 'connection --server …'. O README ensina `cargo build --bin connection` e diz que os pacotes trazem três programas. O pacote seele-cli só leva o `seeled`. O `connection` e a TUI saíram em 31/08. Quem segue a primeira instrução que o servidor mostra bate num erro.
  - *Evidência:* LIDO: seele-server/src/main.rs:91,98; README.md:120,159,164-168,188; docs/teste-duas-maquinas.md:20,79,99; empacotar/macos.sh:189; commit aa77ad8 (ADR 0039). MEDIDO: corpo do release v0.15.0 («Dentro de cada um vão três programas»).
- **[importante · esforço M · risco baixo] No Windows, tela entre duas máquinas, picotado, troca de fone e eco da tela inteira seguem sem confirmação de campo.** Os relatos de campo vêm de casas com Windows. A pendência 33 (tela entre duas máquinas Windows parou na 0.8.5) nunca foi confirmada como resolvida. A 15 (picotado) e a 31 (troca de fone) foram consertadas sem teste de campo. Na R21, o Windows não tira o som do SEELE da captura da tela inteira. Se a sessão de validação mostrar a 33 ainda quebrada, isto vira bloqueante, ou o compartilhamento de tela entre dois Windows sai do anúncio da 1.0 como beta.
  - *Evidência:* LIDO: pendencias.md seções 15, 31 e 33; notas-da-v0.11.0.md:335; entrega-review-v15 (R21 parcial, «o que não está feito» itens 1 e 3); versoes.rs:291-296. Os ramos cfg(windows) (video.rs::som_da_maquina, laco_por_processo.rs) só são compilados pelo job clippy (windows), que está vermelho.
- **[importante · esforço M · risco medio] A bateria de conformidade falha de vez em quando sob carga, e há um provável defeito de produto no meio.** Quando a CI virar portão, falhas intermitentes produzem vermelho falso e ensinam a ignorar o vermelho. A pendência 41 aponta um mecanismo de produto plausível (a limpeza do caminho entre pares da conexão substituída correndo atrás da discagem nova) que ninguém consertou. A 43 dá a receita: dois processos prontos por núcleo reproduzem o SemResposta.
  - *Evidência:* LIDO: pendencias.md seções 40, 41 (2 reprovações em 10 com o binário inteiro) e 43 (2 em 40 com 15 núcleos ocupados); ci.yml:150-200 (a vaga RAII da pendência 29).
- **[importante · esforço M · risco baixo] Nada é assinado (pendência 16).** No primeiro contato, o macOS oferece 'Mover para o Lixo' e o Windows mostra o SmartScreen. Não é trabalho de código: o release.yml já sabe assinar. Depende de comprar a conta da Apple e a assinatura do Azure, e de uma validação de identidade que leva dias. Dá para sair na 1.0 sem assinar, com instrução, mas é a primeira impressão de toda instalação.
  - *Evidência:* LIDO: pendencias.md seção 16; README.md:124-130; ADR 0026; docs/assinatura-e-atualizacao.md.
- **[importante · esforço M · risco medio] A entrada em sala se confirma pelo silêncio (37) e a expulsão nunca foi provada em campo (45): decidir antes de congelar o protocolo.** O conserto de raiz da 37 é uma mensagem nova de protocolo. Se a 1.0 congelar o fio, ou ela entra antes, ou fica para uma 2.0. Cada motivo novo de recusa exige lembrar de desfazer o assento no cliente, e nada no tipo obriga. A terceira metade da 45 (EnterVoiceRoom conferir se a sessão ainda é a vigente) muda a mesma função.
  - *Evidência:* LIDO: pendencias.md seções 37 e 45; seele-proto/src/control.rs:720-724 (só as recusas, nenhuma confirmação); seele-core/src/state.rs:1273-1281.
- **[importante · esforço P · risco baixo] O registro de pendências parou em 18/09 e descreve estados que não são os de hoje.** O índice é da v0.11.0. A pendência 38 diz que o job Windows nunca rodou. A 3 e a 9 falam de programas que saíram. Os itens abertos da v15 (R12, R15, R21 no Windows, F01/F02 acústico, a abertura de áudio sem prazo) não estão lá. A decisão sobre a 1.0 vai se apoiar nesse arquivo.
  - *Evidência:* MEDIDO: git log -- docs/pendencias.md (último commit 2da00a2, de 18/09); grep por R21, F02 e 0.15 no arquivo: nada. LIDO: pendencias.md:11-60.
- **[desejável · esforço M · risco baixo] A suíte verde prova menos do que parece.** 240 dos 400 testes do app só conferem texto. 10 das 14 bancadas de navegador ficam fora da CI. Os testes de vídeo pulam na CI por falta do codec. Não há teste de fumaça do binário distribuído em nenhum sistema (pendência 47). Não bloqueia a 1.0 se houver a sessão de campo; é o que evita que a 1.0.x regrida.
  - *Evidência:* MEDIDO: grep com contagem por arquivo (frontend.rs = 240); ls apps/seele-app/bancada (14 bancadas e 1 auxiliar). LIDO: ci.yml:395-398; ida_e_volta.rs:77; qualidade-do-codec.rs:214; pendencias.md seção 47.
- **[desejável · esforço P · risco baixo] O realce de busca reescreve o texto da mensagem quando os casamentos se sobrepõem (pendência 14).** Buscar 'aa' em 'aaa' mostra 'aaaa': o texto de alguém aparece errado na tela. O conserto é um `if (start < cursor) continue` depois de ler o ordinal.
  - *Evidência:* LIDO: seele-core/src/search.rs:92-105 (janelas sobrepostas); apps/seele-app/ui/tela-sessao.js:1413-1430 (sem guarda).
- **[desejável · esforço P · risco baixo] O número de espectadores não é reanunciado quando alguém passa a assistir (pendência 35).** Um N menor que o real devolve um teto de admissão maior que o real. Só vale com a malha entre pares ligada.
  - *Evidência:* LIDO: seele-server/src/voice_room.rs:821-848 (assistir não chama anunciar_espectadores; as chamadas ficam em 707 e 809).
- **[desejável · esforço G · risco medio] R12 e R15 pela metade: histórico longo sem virtualização; sem painel de armazenamento nem backup.** A cópia do snapshot saiu. Continua valendo: a lista de mensagens é refeita inteira, o critério de 10 mil mensagens nunca foi medido, e não há backup e restauração testados de identidade e servidor. Cabe na 1.0.x para grupos pequenos.
  - *Evidência:* LIDO: entrega-review-v15-2026-09-22.md:34,37,108-114.

### O que o revisor conferiu

- **CONFIRMADA**: 1. Não há execução verde da CI desde 16/08, e o código publicado como v0.15.0 reprovou em 5 de 9 jobs, nos três sistemas.
  - MEDIDO. A API deu 403 por limite de taxa, então li o HTML das páginas de Actions. Em github.com/DATA-AND-DEV/SEELE/actions/workflows/ci.yml?page=1..11 há 268 execuções listadas: 14 'completed successfully', e a última delas é a #28, de 2026-08-16T02:57Z ('ci: cada arquivo publicado leva atestado…'). As execuções #29 a #275 são 219 'failed' e 35 'cancelled'. No run 35762838943 (push de 14d9c30, branch review-v15), as anotações mostram clippy (macos) com exit 101 em 1m46s, test (macos) com exit 101 em 1m48s, clippy (windows) com exit 1 em 4m53s, test (linux) com exit 101 em 4m56s e test (windows) com exit 1 em 14m20s. fmt, regras, bancadas e clippy (linux) passaram. 'git diff --stat 14d9c30 e2fac4d' só toca ci.yml, release.yml, docs e empacotar/notas/0.15.0.md. **Correção:** Os fatos se sustentam, mas faltam três nuances que mudam o diagnóstico. (a) INFERIDO: o macOS entrou na matriz nesse mesmo commit, então a chave de cache era nova. Falhar em menos de 2 minutos com cache frio não dá tempo de compilar o workspace, e nisso o clippy e o test do Mac provavelmente quebraram na montagem ou no ambiente do runner, não em asserção. A árvore-pai 5340290 passou localmente no Mac com 2.314 testes (docs/evidencias/review-v15/validacao.json). (b) clippy (windows), test (windows) e test (linux) já reprovavam em 5340290 (run 35659543590, MEDIDO): são falhas antigas. (c) Entre 1974d07 (02/09, que apagou o ci.yml por custo de minutos) e 13e0add (13/09) a CI não existia. Parte das 'cinco semanas' é ausência de CI, não CI vermelha.
- **PARCIAL**: 2. As versões recentes saem por fora do portão R07: nenhuma passou pelo validar em três sistemas do release.yml.
  - MEDIDO: o corpo do release v0.15.0 (HTML de SEELE-RELEASES/releases/tag/v0.15.0) traz «Fora da integração contínua… a partir do commit e2fac4dab…». A página de runs do Release mostra que a #69 (0018e71, 21/09 21:33) é a última e que as #50 a #69 estão todas 'failed'. Nas anotações do run 35657858980 (0018e71): «Falta: AZURE_ENDPOINT, AZURE_ARTIFACT_SIGNING_ACCOUNT…» e «só metade da chave do atualizador existe; este release não atualiza ninguém». LIDO: publicar.sh:936-1055 é uma bateria que aborta a publicação. No Mac ela roda fmt, clippy com -D warnings, cargo test --test-threads=1, check-deps e cargo deny (licenças e avisos); no Windows, por SSH, roda cargo test --workspace. Ela é chamada em publicar.sh:1613. release.yml:769-782 publica no repositório SEELE, não em SEELE-RELEASES. **Correção:** Que a publicação saiu por fora do R07, confere. Mas não saiu por fora de qualquer portão: desde 1974d07 o caminho de publicação é o publicar.sh, que tem bateria no Mac e no Windows e morre se ela reprovar, exceto com --sem-bateria. O release.yml não é caminho de publicação viável. Os segredos do atualizador e do Azure estão incompletos nas Actions, e ele publica no repositório errado. Os runs 'failed' das 0.14.x não são 'publicou apesar do portão': esse workflow nem era quem publicava. O que ninguém pode verificar é se a v0.15.0 usou --sem-bateria, porque o script sabe e não grava isso no corpo. O critério de refutação («bateria verde nos três sistemas») é impossível por desenho: o Linux nunca roda bateria (publicar.sh:852-854).
- **CONFIRMADA**: 3. Toda subida de protocolo separa quem não atualizou junto; isso aconteceu duas vezes entre versões publicadas em 5 dias; e a saída das notas da 0.15 não existe no app.
  - MEDIDO, com git show por commit de release: e90bcc1 (0.11.2, 18/09) tem PROTOCOL_VERSION=6; fc9146e (0.12.0, 19/09) tem 7; 655a137, a6d5882, 5fe1768, 9755fea e 0018e71 têm 7; e2fac4d (0.15.0, 22/09) tem 8. São sete subidas desde 31/08: 6cf45dc→2, 299339e→3, 1edf3b0→4, 5ad2afa→5, d526b28→6, de10a59→7, 14d9c30→8. LIDO: control.rs:2301 carimba a versão global e control.rs:2326 recusa pelo byte antes de ler o corpo; version.rs:164 fixa a janela em 1. baixar_versao não recebe argumento (main.rs:5497-5531) e chama baixar_a_mais_nova, que usa manifesto.mais_nova() (versoes.rs:321-361). No Windows não há lado a lado (versoes.rs:285-296). MEDIDO: o latest.json da v0.15.0 traz {version:0.15.0, platforms:[darwin-aarch64, windows-x86_64]}. **Correção:** Confirmada, com dois acréscimos. (a) A documentação interna diz o contrário das notas: entrega-review-v15:136-137 e o comentário em session.rs:4216-4218 afirmam que «um par v7 continua entrando, conversando e escrevendo». Isso só vale para um v7 simulado com o mesmo codec; o próprio control.rs:3055-3058 avisa que esse tipo de simulação não mede nada. (b) O seletor «VERSÃO QUE VAI HOSPEDAR» existe (index.html:529). Falta apenas um jeito de obter a 0.14.2, e em Linux e Mac Intel não há pacote nenhum.
- **PARCIAL**: 4. A voz entre duas máquinas com microfone real nunca foi validada por roteiro, e a v0.15 liga por padrão um filtro calibrado só com sinal sintético.
  - LIDO: README.md:288-290. docs/m1-medicoes.md não tem seção M1.15 nem M1.16; só as cita nas linhas 136, 178 e 427. duas_maquinas.rs:46 tem #[ignore] e, mesmo rodando, só testa Client::connect (duas_maquinas.rs:60-70), sem voz. voice.rs:688 define SUPRESSAO_PADRAO=1_000, usado em voice.rs:880. teste-duas-maquinas.md:20 manda compilar '--bin connection', e os únicos [[bin]] do workspace são seeled, seele-encontro, seele-app e os do fuzz. O entrega-review-v15:93-100 confirma: nenhuma validação acústica. **Correção:** 'Nunca validada por roteiro' confere. 'Nunca validada' não: pendencias.md:4065-4067 registra um teste de uso «em LAN entre Mac e Windows» em 31/08, com voz (o defeito relatado foi a troca de fone), e o README:284-286 registra duas máquinas via NAT. O que nunca aconteceu é um registro formal, e nenhuma sessão real depois de F01/F02. Sobre 'o sinal sintético escondeu 5 defeitos': a auditoria achou os cinco também com sinal sintético, trocando o oráculo (entrega:95-99). O defeito estava nos oráculos dos testes. Há atenuantes: o interruptor está na interface (index.html:2653-2654; tela-server.js:556) e a sessão nasce em aperte-para-falar. E um agravante que o analista não citou: a própria revisão v15 (review-v15-2026-09-21.md:268) recomendou que a supressão só entrasse depois da avaliação acústica.
- **CONFIRMADA**: 5. Tolerar mensagem desconhecida é viável sem desalinhar a leitura, porque cada quadro de controle leva o próprio comprimento; encerrar a sessão é escolha de quem recebe.
  - LIDO: seele-core/src/frame.rs:25-43 e seele-server/src/frame.rs:103-131 leem um u32 de comprimento e depois fazem read_exact do quadro inteiro. O leitor do servidor retorna por decisão própria em NaoEntendi (session.rs:1336-1348), e o do cliente retorna em qualquer erro (client.rs:593-603). Já existe precedente de tolerância: fluxos unidirecionais de tipo desconhecido são descartados sem derrubar a conexão (client.rs:658-664; session.rs:1426-1432). O servidor já filtra por vocabulário do par (session.rs:4177-4231, entende_a_mensagem). Não achei caminho de controle fora de frame::read/ler: tela, anexo, volume e par usam fluxos próprios, com byte de tipo. **Correção:** Confirmada, com limites que o desenho precisa cobrir. (a) decode recusa pelo byte de versão antes do postcard (control.rs:2323-2329), então tolerar exige também carimbar a versão negociada ou aceitar carimbo maior. (b) postcard::from_bytes ignora bytes sobrando (postcard 1.1.3, src/de/mod.rs:10-12): um campo acrescentado no fim de uma mensagem de topo é descartado calado, o que é bom. Mas um campo inserido numa struct ou enum aninhada decodifica como lixo plausível, sem erro. (c) Os comentários que dizem «desloca a leitura do fluxo para sempre» (version.rs:116-118; session.rs:4204-4205 e 4213-4214) estão errados diante do prefixo de comprimento: o efeito real é a sessão encerrar.
- **CONFIRMADA**: 6. Ao conectar, o app só avisa que está pronto depois de abrir o áudio, e nenhum dos dois passos tem prazo: um bloqueio do aparelho trava o conectar inteiro.
  - LIDO: seele-ffi/src/lib.rs:3938-3953 chama Voice::start_preferring antes de ready.send (lib.rs:3955). Essa chamada leva a open_preferring e daí a device::open, todos síncronos (voice.rs:641-660 e 988-996; device.rs:665), sem prazo. ready_rx.recv() não tem prazo (lib.rs:1370-1372). No app, spawn_blocking(...).await também não tem prazo (main.rs:1116-1119). Na interface, invoke("connect") não tem prazo e sempre passa audio: true (tela-boot.js:191-204), então não dá para entrar só com texto: resta fechar o app. **Correção:** O código confirma. Já a frase 'bloqueio já observado' é mais fraca do que parece. Os casos registrados são de ambientes atípicos: uma cópia QA com outro identificador e assinatura ad hoc (jornadas-pendentes:81-86) e um shell isolado (jornadas-pendentes:107-110). O review-v15:308 é outro caminho, CapturaDaSaida em laco.rs:580, que abre a saída como entrada, e não device::open. Não achei relato de campo no app normal.

### Severidades contestadas

- **A CI está vermelha nos três sistemas desde 16/08, e ninguém a dispara desde 22/09**: o revisor propõe *importante*. CI vermelha não prejudica um usuário; o que prejudica é publicar código sem teste, e o caminho real de publicação (publicar.sh:936-1055) tem uma bateria que aborta: Mac completo e cargo test no Windows. As duas falhas do Mac morrem em menos de 2 min com cache frio (INFERIDO: runner). O Linux nem é distribuído (a v0.15.0 não tem pacote Linux; MEDIDO). Amarrar a publicação ao release.yml exige segredos que não existem (MEDIDO no run 35657858980) e um repositório de destino que não é o do atualizador. O bloqueante de verdade é bem menor e barato: registrar no corpo do release que a bateria rodou verde no SHA, em Mac e Windows, e rodar clippy no Windows ao menos uma vez. O publicar.sh nunca o roda (publicar.sh:1034-1039), e é plausível que o clippy (windows) vermelho seja lint real em código cfg(windows).
- **Cada subida de protocolo separa o grupo, e a saída que as notas da 0.15 oferecem não existe**: o revisor propõe *bloqueante_1_0 só para o contrato (carimbo negociado + tolerância a variante desconhecida); a promessa falsa das notas é 'importante' e se resolve editando empacotar/notas/0.15.0.md e o corpo do release*. Juntar os dois infla o conserto. A nota errada custa uma edição de texto hoje e não depende da 1.0. O contrato é o que dá sentido à palavra 1.0, e o custo dele é menor do que parece: o servidor já filtra o vocabulário por par (session.rs:4177-4231) e o fio já tolera tipos de fluxo desconhecidos (client.rs:658-664). Um agravante que o analista não viu: a busca por atualização só acontece quando alguém aperta o botão (tela-server.js:1435, nenhuma chamada na abertura). Hoje, o primeiro sinal de versão nova que alguém recebe é «VERSÃO INCOMPATÍVEL».

### O que o analista não viu

- O release.yml não consegue produzir um release utilizável, então o portão R07 está num caminho que não publica. MEDIDO: nos runs #50 a #69 do Release, todos 'failed', as anotações do run 35657858980 dizem «Falta: AZURE_ENDPOINT…» e «só metade da chave do atualizador existe; este release não atualiza ninguém». LIDO: ele faz 'gh release create' no repositório SEELE (release.yml:769-782), enquanto o endpoint primário do atualizador é o SEELE-RELEASES (tauri.conf.json:58-61). O docs/entrega-review-v15:21 marca R07 como 'feito' sobre esse workflow.
- O produto sabe e não conta, aplicado à publicação: o publicar.sh sabe se rodou com --sem-bateria (publicar.sh:937-940) e não escreve isso no corpo do release (bloco em publicar.sh:1855-1880). O próprio script registra que a 0.10.1-1 e a 0.10.3 saíram sem teste e voltaram como relato de campo (publicar.sh:887-889). Gravar 'bateria: Mac ok, Windows ok, SHA X' no corpo é o jeito barato de atender o item (1) do analista.
- O corpo do release v0.15.0 afirma uma coisa falsa. MEDIDO: «Esta versão não traz nenhuma mudança de produto: só documentação, testes e arrumação interna», logo abaixo das notas que anunciam protocolo 8 e redução de ruído. LIDO: o classificador só aceita os prefixos feat:, fix: e perf: (publicar.sh:516-531) e descarta todo o resto com 'continue'. Os commits deste repositório passaram a ter assunto em português sem prefixo, então todo commit de produto some. O guarda xtask/tests/empacotamento.rs:1763 confere que essa frase existe, e assim prende a saída errada.
- A v0.15.0 não tem pacote para Linux nem para Mac Intel. MEDIDO: os arquivos do release são .dmg e .app.tar.gz aarch64, o .exe x64 e as ferramentas de terminal, e o latest.json só traz darwin-aarch64 e windows-x86_64. A CI gasta um job de teste em Linux que nenhum usuário recebe. A 1.0 precisa declarar quais plataformas suporta antes de exigir CI verde nelas.
- Não há gancho de pânico no app. Não existe std::panic::set_hook fora de testes (a busca só acha crates/seele-conformance/tests/troca_de_aparelho.rs), e o release do Windows roda com windows_subsystem = "windows" (main.rs:22). Um pânico nas threads 'seele-voice' ou de conexão some sem deixar linha no seele.log. O áudio para calado e ninguém fica sabendo por quê.
- Assimetria no 'não entendi': o servidor separa 'fechou' de 'não entendi' e avisa com warn (seele-server/src/frame.rs:61-80; session.rs:1336-1348). O cliente trata os dois igual, com debug «o fluxo de controle terminou» (seele-core/src/client.rs:599-602; seele-core/src/frame.rs sem FimDoQuadro). Uma incompatibilidade no meio da sessão fica calada no lado de quem usa.
- O guarda do fio que o analista marcou como 'ausente' existe em parte: control.rs:4492-4530 prende ordinais e a contagem de variantes de topo (36 em ClientMessage, 40 em ServerMessage). Mas os enums aninhados (AlertReason, DisconnectReason, MessageRefusal) não têm guarda nenhum: a v8 acrescentou AlertReason::RetencaoApagouHistorico sem nada disparar (só existem ultima_variante::<ClientMessage> e ::<ServerMessage>). E a_release_publicada_recusa_o_carimbo_desta_build (control.rs:3059-3074) usa o fixo PUBLICADA=3 (v0.10.5-1): passa no vazio e já não diz nada sobre a v7 publicada. É o caso de existir sem funcionar.
- O registro de pendências está atrasado de um jeito que engana. O índice ainda diz «38 — o job windows-2022 nunca rodou» (docs/pendencias.md:37-45), mas ele roda e reprova desde 18/09 (MEDIDO: runs #150 a #275). Quem lê o índice conclui que o Windows nunca foi exercitado automaticamente, quando na verdade ele reprova de forma conhecida e sem diagnóstico.

### Desenho proposto pelo analista

## O mínimo para chamar de 1.0, em ordem

**Etapa 0: saber por que a CI está vermelha (P a M, 1 a 2 dias).**
1. O dono abre, logado, os logs dos 5 jobs vermelhos do run 35762838943 (sem login a API devolve 403).
2. Classificar cada falha como produto, ambiente ou teste frágil:
   - produto: consertar;
   - ambiente: pular **por nome, com o motivo escrito**, no mesmo padrão que o `ci.yml` já usa para `a_saida_desta_maquina_abre_como_entrada`;
   - para o macOS com cache frio, conferir o que falha nos primeiros 85 s.
3. Devolver pelo menos `push` na `main`. As duas linhas estão no próprio cabeçalho do `ci.yml`.
4. Critério de saída: uma execução verde nos três sistemas sobre o mesmo SHA.

**Etapa 1: amarrar a publicação real à CI (P).**
- `publicar.sh --conferir` consulta `GET /repos/DATA-AND-DEV/SEELE/commits/<sha>/check-runs`, com o token que o script já usa, e recusa se o SHA não tiver `test` e `clippy` verdes nos três sistemas.
- A saída de emergência fica explícita e é escrita no corpo do release, como a frase «Fora da integração contínua» já é.
- Alinhar o clippy local com o da CI (`--all-features`).
- Reaproveita `conferir_arvore`, o token e o texto de procedência que já existem.

**Etapa 2: o contrato de fio da 1.0 (M). Tem de vir antes da sessão de campo, porque a sessão valida o binário congelado.**
1. Listar o que ainda precisa entrar no fio antes de congelar:
   - a confirmação de entrada em sala (pendências 37 e 45);
   - o que as outras frentes precisarem para link persistente, vários servidores e chamada privada.
   Depois da 1.0, o que ficar de fora só entra de forma aditiva.
2. Uma última subida de protocolo, a «v9 = 1.0», com:
   - (a) quem recebe um quadro com variante desconhecida **ignora, conta e registra uma vez**, em vez de encerrar a sessão. Hoje o servidor encerra em `session.rs:1336-1343`; o cliente precisa do mesmo. Isso é viável porque cada quadro leva o próprio comprimento (`frame.rs:29-43` nos dois lados), então pular um quadro não estraga os seguintes;
   - (b) carimbo com a versão negociada, e não a global (`control.rs:2298`), ou aceitação de carimbos dentro da mesma versão maior;
   - (c) cada mensagem nova anunciada como capacidade no `Hello`.
3. Guarda: `crates/seele-proto/tests/fio_1_0.rs` com os bytes de cada `ClientMessage`, `ServerMessage` e `Event` existente, que reprova se a codificação mudar; mais um teste que reprova se `PROTOCOL_VERSION` mudar sem subir a versão maior. Provar o guarda revertendo e vendo reprovar.
4. Política escrita num ADR e em `specs/10`: «1.0.x nunca quebra o fio».
5. Corrigir as notas da 0.15, porque a saída lado a lado não existe. Se o dono quiser manter a promessa, publicar o `latest.json` com várias versões (schema 2), que `seele-lancador/src/manifesto.rs` já lê, e trocar `baixar_versao` por «baixar a versão X».

**Etapa 3: sessão de campo registrada (M, duas sessões de 45 min, depende da Etapa 2).**
- Montagens: Windows↔Windows e Windows↔Mac, com o binário 1.0-rc e o roteiro corrigido (trocar `connection` por `seele-app` e `seeled` em `docs/teste-duas-maquinas.md`).
- Cobrir:
  - 30 minutos de voz com a redução de ruído no padrão: fala baixa, ventilador, teclado, fone e microfone de notebook (F01/F02);
  - trocar o fone no meio (pendência 31);
  - tela Windows↔Windows e Mac→Windows (33); tela inteira com som no Windows (R21);
  - expulsar (45);
  - derrubar e voltar a rede.
- Registrar em `docs/m1-medicoes.md` (M1.15 e M1.16).
- Se a supressão soar mal, baixar `SUPRESSAO_PADRAO` (`voice.rs:688`) antes da 1.0.
- O que falhar vira conserto ou sai do anúncio (por exemplo, «tela entre dois Windows: beta»).

**Etapa 4: quatro consertos de 'sabe e não conta' (P cada um, em paralelo com a 2).**
1. `admissao.rs:85-101`: trocar `.ok()` e `.unwrap_or(0)` por `.optional()?`, como `portaria.rs:118-128`, com um teste de banco que falha provando a recusa.
2. Prazo na abertura de áudio.
   - A thread de voz passa a abrir o próprio aparelho, porque os streams do cpal não podem mudar de thread (`voice.rs:950`), e responde por canal.
   - Quem espera aplica um prazo de cerca de 5 s. Estourou: segue só com texto, pelo braço que já existe (`lib.rs:3951`), com a frase nomeada. O `ready` deixa de depender do áudio.
3. «COPIAR DIAGNÓSTICO» e «ABRIR PASTA DO LOG» em CONFIGURAÇÕES, levando junto `drops()`, `atrasos` e as medidas do laço de voz.
4. Tirar o `connection` de `seeled/main.rs:91,98`, do README, do roteiro e do texto do release.

**Etapa 5: higiene (P).** `pendencias.md` ganha um índice da 0.15:
- os itens R12, R15, R21 (Windows), F01 e F02, e a abertura de áudio sem prazo;
- a 38 reescrita com o estado medido;
- a 3 e a 9 marcadas como obsoletas.

**Cadência de saída.** Um `1.0.0-rc.1` com o protocolo congelado e 3 a 7 dias de uso pelo grupo sem nenhuma mudança de fio, antes do `1.0.0`. A 1.0 sai de um SHA com CI verde e com a sessão de campo registrada.

## O que pode ficar para 1.0.x
- Assinatura (pendência 16): é compra, e dá para sair sem ela com instrução clara.
- Aviso do sistema com a janela fechada (23) e `seele://` clicável (10).
- R12 (virtualização do histórico) e R15 (painel e backup).
- Pendências 14, 35, 34, 6, 8, 26 a 28, 32 e 36.
- 46: sai quando o job Windows ficar verde.
- 47: teste de fumaça nativo por sistema na 1.0.1.
- As outras 10 bancadas na CI.
- A intermitência das pendências 41 e 43, se ela não impedir o verde da Etapa 0.
- Linux e `.deb` (48).
- Vários servidores ao mesmo tempo (24), se couber de forma aditiva no fio — as outras frentes decidem.

## Dependências
- A Etapa 0 vem antes da 1.
- A Etapa 2, com as decisões de protocolo das outras frentes, vem antes do congelamento.
- A Etapa 3 valida o rc que sai da 2.
- A Etapa 4 corre em paralelo e precisa entrar no rc.

## O que se reaproveita
Vaga RAII da pendência 29, o pulo por nome no `ci.yml`, `publicar.sh`, o manifesto schema 2 do lançador, o quadro com comprimento na frente, o braço «só texto» da FFI, o padrão `.optional()?` da portaria, e o log com rotação.

### Perguntas ao dono

- Você consegue abrir, logado, os logs dos 5 jobs vermelhos do run 35762838943 (CI de 14d9c30, o mesmo código da v0.15.0)? Sem login a API devolve 403. As falhas são do produto, do ambiente ou de teste frágil?
- A v0.15.0 foi publicada com o publicar.sh rodando a bateria completa (Mac e Windows por SSH) ou com --sem-bateria? O teste a_saida_desta_maquina_abre_como_entrada terminou no seu Mac?
- Por que a CI foi tirada do push: custo de minutos ou barulho? Aceita devolver pelo menos o push na main para a 1.0?
- Aceita congelar o protocolo na 1.0, com nenhuma 1.0.x mudando PROTOCOL_VERSION? Se sim, o que precisa entrar no fio antes: confirmação de entrada em sala (pendências 37 e 45), link persistente, vários servidores ao mesmo tempo, chamada privada?
- Quer manter a promessa de guardar uma versão antiga ao lado (inclusive no Windows)? Se não, as notas da 0.15 precisam ser corrigidas, porque hoje o app não consegue baixar a 0.14.2.
- Quais sistemas a 1.0 promete: só Windows x64 e macOS Apple Silicon? Linux e Mac Intel ficam de fora do anúncio?
- Há 2 ou 3 pessoas disponíveis para duas sessões de uns 45 min (Windows↔Windows e Windows↔Mac) com o roteiro corrigido, antes da 1.0?
- Se essa sessão não acontecer antes da 1.0, aceita baixar ou desligar por padrão a redução de ruído, que hoje vai na força máxima sem ter sido ouvida em voz de gente?
- A tela entre duas máquinas Windows (pendência 33) voltou a funcionar em alguma versão depois da 0.8.5, pelo que você ou seus amigos viram?
- Há orçamento e prazo para a Apple Developer e o Azure Artifact Signing antes da 1.0, ou a 1.0 sai sem assinatura, com instrução?

---

## Segurança e privacidade

**Resumo do analista.** A base de segurança do SEELE é mais cuidadosa que a média: TLS 1.3 em tudo, portaria ligada por padrão no HOSPEDAR AQUI, prévias de link buscadas pelo cliente só com consentimento por domínio e com proteção contra SSRF, MODs de cliente em QuickJS vindos só do catálogo assinado, atualizador com minisign, e `cargo deny check advisories` limpo hoje (MEDIDO; o rustls 0.23.45 já traz a correção da RUSTSEC-2026-0285). Três falhas, porém, tornam arriscado lançar a 1.0 para estranhos, e todas se confirmam lendo o código. (1) O nome de arquivo que o remetente escolhe vira caminho no disco de quem clica em SALVAR, e isso permite sobrescrever qualquer arquivo da pasta pessoal. (2) A prova de identidade assina só um nonce, sem nada que a amarre ao servidor, e a impressão digital (fp) do link é conferida depois de o segredo e a assinatura já terem sido entregues: qualquer servidor malicioso consegue se passar por você em outro servidor onde você já foi admitido, inclusive como Comandante do seu próprio servidor. (3) A reconexão pela lista, que é a base da «URL que não muda», não confere a fp guardada, e o quarto do ponto de encontro pode ser ocupado por qualquer um que tenha o link. Com isso, quem volta ao servidor pode cair num impostor e só vê um aviso de PRIMEIRO CONTATO. Fora isso há lacunas importantes: nada roda automaticamente (o CI é manual, o fuzz está parado desde 07/08, e o repositório agora é público, então os minutos são grátis), os binários não têm assinatura de sistema, o install.sh instala a v0.10.0, o seeled nasce aberto e o primeiro a entrar vira Comandante, a fila da portaria pode ser inundada, o ponto de encontro não tem limitação de taxa nem aviso de privacidade, e não há caminho na interface para aceitar uma troca legítima de chave. Recomendação: corrigir os três bloqueantes (1 fica em horas; 2 e 3 em dias, com o protocolo subindo para 9, o que a v0.15 já mostrou ser aceitável), religar a CI com deny e fuzz, e só então cortar a 1.0.

### O que existe

- `construido_e_publicado`: TLS 1.3 via QUIC com TOFU e pin por endereço; chave trocada derruba no TLS (crates/seele-core/src/tofu.rs:246-287; crates/seele-ffi/src/lib.rs:3729)
- `parcial`: Conferência da fp do link (FirstContactVerified / InviteRefused) (crates/seele-core/src/enlace.rs:4607-4650)
- `parcial`: Desafio e resposta Ed25519 com identidade em disco (0600) (crates/seele-core/src/client.rs:1656-1706; crates/seele-server/src/session.rs:560-599; crates/seele-core/src/identity.rs:87-116)
- `construido_e_publicado`: Convite de uso único e senha Argon2id, expostos no app (crates/seele-server/src/admissao.rs; apps/seele-app/src/main.rs (definir_senha_do_server, criar_convite_do_server))
- `construido_e_publicado`: Portaria ligada por semente no HOSPEDAR AQUI, com o dono pré-admitido (apps/seele-app/src/main.rs:1582-1607; crates/seele-server/src/portaria.rs)
- `ausente`: Limpeza e teto da fila de pedidos da portaria (docs/adr/0030-quem-bate-a-porta.md:16-17)
- `construido_e_publicado`: Limitação de taxa em dois baldes (por IP antes, por conexão depois) (crates/seele-server/src/taxa.rs)
- `construido_e_publicado`: Aviso de servidor aberto no seeled (crates/seele-server/src/main.rs:109-117)
- `construido_e_publicado`: cargo deny (licenses + advisories) na bateria de release (empacotar/publicar.sh:993-1014)
- `ausente`: cargo deny / cargo audit / fuzz no CI (.github/workflows/ci.yml (removidos em 1974d07; só workflow_dispatch))
- `construido_mas_nao_ligado`: Alvos de fuzz de seele-proto (fuzz/fuzz_targets/*.rs (corpus de 07/08 com versão 0/1))
- `construido_e_publicado`: Executor de MOD de cliente em QuickJS com tetos (apps/seele-app/src/executor.rs; apps/seele-app/ui/mods-runtime.js:50-66)
- `construido_e_publicado`: Catálogo de MODs assinado (minisign, chave embutida separada) (apps/seele-app/src/catalogo.rs; apps/seele-app/chaves/mods.pub)
- `construido_e_publicado`: MOD de servidor: disco limitado à pasta do MOD, rede de saída livre (crates/seele-server/src/mods/arquivos.rs:36-55; mods/mundo.rs:40-95)
- `construido_e_publicado`: Prévia de link pelo cliente, com consentimento por domínio e bloqueio de endereço reservado (apps/seele-app/src/main.rs:4176-4480; crates/seele-core/src/preferences.rs:505)
- `construido_e_publicado`: Anexos por hash no servidor, cota, prévia só pelos bytes, quarentena ao salvar (crates/seele-server/src/persistence/attachments.rs; crates/seele-core/src/client.rs:352-385)
- `ausente`: Nome de arquivo seguro ao salvar no cliente (apps/seele-app/ui/tela-sessao.js:3303; crates/seele-core/src/client.rs:2114)
- `construido_e_publicado`: Atualizador com assinatura minisign do pacote (apps/seele-app/tauri.conf.json (plugins.updater); crates/seele-lancador/src/assinatura.rs)
- `ausente`: Rotação e reserva da chave do atualizador (docs/assinatura-e-atualizacao.md:154-158)
- `construido_mas_nao_ligado`: Revogação assinada de versões (crates/seele-lancador/src/revogacao.rs e manifesto.rs:250-304 (não emitida por empacotar/manifesto.py; não consultada em apps/seele-app/src/versoes.rs:219))
- `construido_mas_nao_ligado`: Assinatura de SO (Developer ID + notarização; Azure Artifact Signing) (.github/workflows/release.yml:277-393 (só se houver segredos); tauri.conf.json signingIdentity "-")
- `parcial`: install.sh / install.ps1 com conferência SHA256 (install.sh:31,97-120; install.ps1:28 (apontam para o repositório cujo latest é a v0.10.0))
- `construido_e_publicado`: Ponto de encontro com quarto (MORO/QUEM) e reflexão 1:1 (crates/seele-encontro/src/lib.rs; crates/seele-proto/src/encontro.rs)
- `ausente`: Autenticação do MORO / detecção de ocupação / limitação de taxa no ponto de encontro (crates/seele-encontro/src/lib.rs:123-131,281-285)
- `ausente`: Caminho na interface para aceitar troca legítima de chave (apps/seele-app/ui/frases.js:411-417; main.rs:6871-6877 (esquecer não apaga pin))
- `ausente`: SECURITY.md / canal de reporte de vulnerabilidade (raiz do repositório)
- `so_desenho`: Chamada privada / E2EE (specs/08-seguranca.md:55-65 (esboço pós-v1))

### Lacunas

- **[BLOQUEANTE · esforço P · risco baixo] Salvar anexo grava onde o remetente quiser (path traversal no cliente).** O `file_name` de um anexo é escolhido por quem envia. Ele é concatenado à pasta de Downloads na UI e vai sem normalização até um `File::create` que cria ou trunca o arquivo. Um membro qualquer manda `../.zshrc`, `../Library/LaunchAgents/x.plist` ou, no Windows, `..\AppData\Roaming\Microsoft\Windows\Start Menu\Programs\Startup\x.bat`. Quem clica em SALVAR sobrescreve o arquivo e ganha execução no próximo login ou terminal. A confirmação mostra o caminho, mas um nome de até 255 bytes com espaços e caracteres bidi o disfarça. A quarentena não impede o shell nem o launchd de ler o arquivo. A própria doc do protocolo afirma que o nome nunca chega ao disco, o que é verdade só no servidor.
  - *Evidência:* apps/seele-app/ui/tela-sessao.js:3127 (dataset.anexoNome = anexo.file_name), :3303 (destino = pasta/nome); apps/seele-app/src/main.rs:2717-2723 (repassa destino); crates/seele-core/src/client.rs:2114 (File::create); crates/seele-proto/src/attachment.rs:76-80,126,297-310 (só recusa vazio/NUL; teste carrega ../../etc/passwd). LIDO.
- **[BLOQUEANTE · esforço M · risco medio] Servidor malicioso se passa por você em outro servidor (relay do desafio) e a fp do link é conferida tarde demais.** O cliente assina o nonce cru que o servidor mandar: sem a fp do servidor, sem vínculo ao canal TLS e sem separação de domínio. Quem já foi admitido pela portaria entra sem segredo. Com isso, qualquer servidor M em que você entra pode abrir uma conexão com X como você, repassar o nonce de X e usar a sua assinatura para entrar em X com a sua conta e os seus papéis. Isso inclui Comandante do seu próprio servidor, onde o dono é pré-admitido. Além disso, o `TofuVerifier` aceita qualquer certificado no primeiro contato, e a conferência contra a fp do link só acontece depois de o `Hello` (com convite ou senha) e a assinatura terem saído. Um intermediário no primeiro contato colhe o segredo e a prova mesmo quando o link traz a fp. É o ponto em que o SEELE entrega uma credencial sem saber a quem, e a 1.0 é justamente entrar pelo link de estranhos.
  - *Evidência:* crates/seele-core/src/client.rs:1663-1672 (Hello com join_secret), :1700 (sign(&nonce)); crates/seele-server/src/session.rs:560,595 (nonce aleatório, verify só do nonce), :632 (ja_admitido dispensa segredo); crates/seele-proto/src/control.rs:2719 (nonce de tamanho variável); crates/seele-core/src/tofu.rs:227,279-281 (verificador sem fp esperada); crates/seele-core/src/enlace.rs:1265-1291 (conferir depois de connect_por); apps/seele-app/src/main.rs:1606; crates/seele-server/src/permissions.rs:330-370. Mecanismo LIDO, ataque INFERIDO (sem teste de conformidade de relay).
- **[BLOQUEANTE · esforço M · risco medio] «Link que não muda» sem conferência: a reconexão pela lista aceita o impostor do quarto.** Quem volta pela lista conecta sem `expected_fingerprint`: a fp guardada serve só para perguntar QUEM. A resposta do ponto de encontro entra na frente dos candidatos, e como o pin é por endereço, o endereço novo é fixado às cegas. A tela mostra só a faixa PRIMEIRO CONTATO. O quarto não tem autenticação: a marca está no link, e quem registra primeiro fica. Basta o anfitrião ficar fora mais de 60 s para qualquer um ocupar a marca. Mantendo o registro vivo, o anfitrião legítimo é recusado sem saber (o MORO é respondido como ONDE). Todos os que voltam caem no impostor, que pode encenar o servidor ou repassar a autenticação (lacuna anterior). É exatamente o caminho da feature de persistência pedida pelo dono.
  - *Evidência:* apps/seele-app/src/main.rs:928-946 (esperada só do convite), :966-986 (impressao_guardada), :1006-1030 (QUEM na frente), :1079 (expected_fingerprint: esperada); apps/seele-app/ui/camada-servidores.js:150-153; crates/seele-ffi/src/lib.rs:3801-3806 (pin por endereço); crates/seele-encontro/src/lib.rs:123-131,281-285; crates/seele-server/src/alcance/encontro.rs:359-376; apps/seele-app/ui/tela-sessao.js:235-241. LIDO + INFERIDO.
- **[importante · esforço P · risco baixo] Nada roda sozinho: CI manual, deny só no release, fuzz parado com corpus morto.** O `ci.yml` só dispara à mão. Os jobs de deny, audit e fuzz saíram em 1974d07 porque os minutos custavam dinheiro em repositório privado, e o repositório agora é público: os minutos são grátis. Uma advisory publicada entre dois releases não é vista por ninguém. O fuzz não roda desde 07/08, e o corpus inteiro tem byte de versão 0 ou 1 enquanto o protocolo é 8, então é recusado na primeira comparação. Ficam sem alvo as superfícies novas que recebem bytes de terceiros: o link `seele://`, o UDP do ponto de encontro, o cabeçalho de anexo, o manifesto de MOD e os cabeçalhos de tela.
  - *Evidência:* .github/workflows/ci.yml:26-27; git show 1974d07 (remove supply-chain e fuzz smoke); empacotar/publicar.sh:993-1014; MEDIDO: API GitHub private=False; MEDIDO: primeiro byte do corpus 0x00/0x01; crates/seele-proto/src/version.rs:124,164; fuzz/artifacts/control_frame/crash-991c… de 07/08.
- **[importante · esforço M · risco baixo] Binários sem assinatura de SO, e o README ensina a tirar a quarentena.** No macOS a assinatura é ad-hoc, sem hardened runtime e sem notarização; no Windows não há Authenticode. Para gente estranha, o primeiro contato com o produto é o Gatekeeper ou o SmartScreen, e a instrução oficial é `xattr -dr com.apple.quarantine`, um hábito que desarma a proteção para qualquer outro app. Os ganchos existem no release.yml e só esperam segredos.
  - *Evidência:* apps/seele-app/tauri.conf.json (signingIdentity "-", hardenedRuntime false); README.md:124-130; .github/workflows/release.yml:277-393; ADR 0026.
- **[importante · esforço M · risco medio] Chave do atualizador sem reserva, e revogação de versões não ligada.** Perder ou vazar a chave minisign obriga todos a reinstalar à mão. O launcher do ADR 0046 mantém toda versão executável, e a revogação assinada existe só do lado do leitor: `manifesto.py` não a emite e o app não a consulta. Depois de corrigir os bloqueantes, as versões ≤0.15 continuam instaláveis e abríveis sem aviso. O segundo endpoint do atualizador aponta para um repositório parado na v0.10.0.
  - *Evidência:* docs/assinatura-e-atualizacao.md:154-158; crates/seele-lancador/src/revogacao.rs, manifesto.rs:250-304; apps/seele-app/src/versoes.rs:219 (VersaoResolvida montada direto); grep MEDIDO sem 'revoc' em empacotar/; tauri.conf.json endpoints; MEDIDO releases/latest de DATA-AND-DEV/SEELE = v0.10.0.
- **[importante · esforço P · risco baixo] install.sh e install.ps1 instalam a v0.10.0 (e falham no Linux).** Os scripts leem `releases/latest` de DATA-AND-DEV/SEELE, que parou na v0.10.0 (02/09) e não tem pacote Linux; os releases de verdade estão em SEELE-RELEASES. A linha do README instala um seeled cinco versões atrás, incompatível com o app atual, sem dizer nada. O SHA256SUMS vem da mesma origem dos binários e só prova que o arquivo chegou inteiro. O README ainda promete `.deb`, que a v0.15.0 não publica.
  - *Evidência:* install.sh:31; install.ps1:28; MEDIDO: API releases/latest de DATA-AND-DEV/SEELE → v0.10.0 sem asset linux; MEDIDO: assets da v0.15.0 sem linux/.deb; README.md:120,171.
- **[importante · esforço P · risco medio] seeled nasce aberto e o primeiro a conectar vira Comandante.** Quem sobe o `seeled` numa VPS e ainda não conectou deixa o posto de Comandante para o primeiro estranho que alcançar a porta 8383. O aviso vai para um stdout que um serviço do systemd nem mostra.
  - *Evidência:* crates/seele-server/src/main.rs:109-117; crates/seele-server/src/lib.rs:271 ([::]:8383); crates/seele-server/src/permissions.rs:330-370; ADR 0030 (portaria desligada por padrão no seeled).
- **[importante · esforço P · risco baixo] A portaria pode ser inundada, e a tabela cheia barra quem é legítimo.** O balde é por endereço IP inteiro. Com IPv6, girar dentro de um /64 contorna o limite, e cada chave nova é de graça e vira uma linha pendente, sem teto e sem limpeza. Passadas 4096 origens, a tabela de baldes recusa todo endereço novo, inclusive o amigo que chega. Não há notificação do sistema, então o anfitrião nem sabe que há fila.
  - *Evidência:* crates/seele-server/src/taxa.rs:54-71,218-255; crates/seele-server/src/portaria.rs:274-281,442-470; docs/adr/0030-quem-bate-a-porta.md:16-17.
- **[importante · esforço M · risco baixo] Ponto de encontro sem limitação de taxa, e metadado sem aviso de privacidade.** Sem limitação por origem, cerca de 70 MORO por segundo mantêm as 4096 marcas cheias e novos anfitriões são recusados em silêncio: um DoS barato da persistência. O operador (o projeto) aprende quem está no ar, em qual IP, e quem procura quem (QUEM e LEVE). Isso é dado pessoal na LGPD, e o README diz que o ponto «esquece» enquanto a spec diz que o produto não fala com terceiros. Faltam uma política de privacidade para encontro.seele.app.br e mods.seele.app.br e um SECURITY.md.
  - *Evidência:* crates/seele-encontro/src/lib.rs:70-84,281-285 (sem balde; grep MEDIDO); crates/seele-proto/src/encontro.rs:52-57; README.md:104-106; specs/08-seguranca.md:109; crates/seele-server/src/alcance/encontro.rs:67.
- **[importante · esforço P · risco medio] Troca legítima de chave do servidor tranca os amigos para fora.** Quando o anfitrião reinstala ou perde a pasta no mesmo IP e porta (8383 via UPnP), cada amigo recebe A CHAVE DO SERVIDOR MUDOU, sem botão para aceitar. `esquecer` não apaga o pin; a saída é editar `pins` à mão. Um link novo com a fp nova, que é a prova natural, também não resolve, porque `Changed` é recusado com ou sem convite.
  - *Evidência:* apps/seele-app/ui/frases.js:411-417; apps/seele-app/src/main.rs:6871-6877; crates/seele-core/src/conhecidos.rs:344; crates/seele-core/src/tofu.rs:138-146,284-287.
- **[importante · esforço P · risco baixo] Não está escrito que quem hospeda lê tudo e pode ouvir.** A spec exige documentar que, na v1, o operador pode capturar mídia e histórico, e o README não diz isso. «Chamadas privadas» sem essa definição viram uma promessa falsa: o SFU encaminha o áudio em claro para quem hospeda.
  - *Evidência:* specs/08-seguranca.md:16-17,65; README.md:5-9.
- **[desejável · esforço G · risco alto] Uma identidade por máquina em todos os servidores.** A mesma chave pública e o mesmo sufixo `pessoa-XXXX` aparecem em toda comunidade, o que permite correlacionar a pessoa entre servidores. Uma chave por servidor também eliminaria o relay entre servidores.
  - *Evidência:* docs/adr/0017-identidade-e-pins-em-disco.md; apps/seele-app/src/main.rs:1034-1062 (apelido derivado da impressão desta máquina).
- **[desejável · esforço P · risco baixo] Banco do servidor (chave TLS, histórico, hash da senha) sem permissão restrita.** A chave privada TLS vive no SQLite, que nasce com a permissão padrão. Numa máquina multiusuário ou num seeled com home 0755, outro usuário lê a chave e as conversas.
  - *Evidência:* crates/seele-server/src/tls.rs:140-175; grep MEDIDO sem set_permissions em crates/seele-server.
- **[desejável · esforço P · risco baixo] Defesa em profundidade na janela: comandos aceitam caminho do JS, nomes aceitam bidi e controle.** Com `withGlobalTauri: true`, qualquer script na janela chama cerca de 150 comandos, e `salvar_anexo` aceita um caminho arbitrário. A CSP e a ausência de innerHTML seguram hoje, mas o Rust deveria decidir o destino. Nomes de arquivo e apelidos aceitam RLO e caracteres de controle, o que permite disfarçar extensão. A CSP ainda tem `worker-src blob:` do executor removido, e os cabeçalhos de executor.rs e mods.rs estão desatualizados.
  - *Evidência:* apps/seele-app/tauri.conf.json; apps/seele-app/src/main.rs:2717-2723; crates/seele-proto/src/attachment.rs:126; apps/seele-app/src/executor.rs:28-33; apps/seele-app/src/mods.rs:1-10.
- **[desejável · esforço P · risco medio] MOD de servidor alcança a rede local de quem hospeda.** Pelo desenho do ADR 0045, `mundo.buscar` não filtra endereço reservado e segue redirecionamento. Um MOD de catálogo ou instalado de pasta local pode varrer o roteador e os serviços locais do anfitrião. A tela de aceite deveria dizer «alcança a sua rede local», ou o filtro de `endereco_de_previa` deveria ser reusado com uma exceção explícita.
  - *Evidência:* crates/seele-server/src/mods/mundo.rs:40-95; crates/seele-proto/src/mods.rs:599-609 (no_servidor).
- **[desejável · esforço P · risco baixo] SHA256SUMS e CLIs sem assinatura do projeto; chave TLS do servidor sem rotação.** O install.sh confere a soma contra um arquivo da mesma origem. Assinar o SHA256SUMS com minisign, ou verificar o atestado do GitHub, fecharia o caso de release adulterado. A chave do servidor não tem caminho de rotação.
  - *Evidência:* install.sh:104-120; MEDIDO: SHA256SUMS da v0.15.0 sem .sig; crates/seele-server/src/tls.rs:66-90.

### O que o revisor conferiu

- **CONFIRMADA**: 1. Salvar um anexo grava em <Downloads>/<file_name do remetente> sem normalizar; um ../ escreve fora da pasta e trunca o arquivo existente.
  - LIDO, caminho inteiro sem nenhum filtro: tela-sessao.js:3127 (dataset.anexoNome = anexo.file_name) → :4059 (salvarAnexo(..., salvar.dataset.anexoNome)) → :3303 (destino = `${pastaDeDestino}/${nome}`) → main.rs:2717-2723 (salvar_anexo repassa destino) → seele-ffi/src/lib.rs:1460-1468 (PathBuf::from(destination)) → seele-core/src/enlace.rs:2963-2976 (SalvarAnexo → receive_attachment) → client.rs:2114 (tokio::fs::File::create(destination), que cria ou trunca). A validação só recusa nome vazio ou com NUL (seele-proto/src/attachment.rs:126), e o teste :297-310 afirma que ../../etc/passwd passa. O servidor também aceita (persistence/attachments.rs:1388: «names are columns»). Não há App Sandbox: Entitlements.plist só declara audio-input, e signingIdentity é "-". **Correção:** Duas nuances. (a) No macOS e no Linux, cada componente antes de um .. precisa existir, então disfarçar com prefixo falso não funciona. Sobra ../.zshrc ou ././…/../x, e a confirmação mostra o caminho inteiro (tela-sessao.js:3306). O disfarce por bidi que o analista cita é exagerado nesses sistemas. No Windows a normalização Win32 é léxica, então componentes inventados antes de ..\ funcionam (INFERIDO). (b) Mesmo com um nome benigno, File::create sobrescreve sem avisar um arquivo de mesmo nome em Downloads.
- **CONFIRMADA**: 2. A prova de identidade assina só o nonce do servidor, sem vínculo ao servidor, e uma chave já admitida entra sem segredo; um servidor malicioso pode repassar o desafio e entrar em outro como a vítima.
  - LIDO: client.rs:1700 (signing_key.sign(&nonce), o nonce cru que chegou); session.rs:560 (nonce aleatório) e :595 (verify só sobre o nonce); session.rs:632 (ja_admitido perdoa segredo ausente); control.rs:273 e :2719 (o nonce aceita até 256 bytes quaisquer); tls.rs:113 (servidor with_no_client_auth, sem canal amarrado). Não há exporter nem transcript: um grep por export_keying_material/channel_binding não acha nada. A chave é uma só para todos os servidores (ffi/lib.rs:248, identity.key), e é o único .sign() com a identidade no repositório. Não achei teste de relay em seele-conformance. O ataque em si é INFERIDO. **Correção:** O subcaso «Comandante do seu próprio servidor» é restrito HOJE. No app, sair do servidor que se hospeda derruba a hospedagem (main.rs:2005-2018), e a multiconexão não existe (main.rs:899). Então, enquanto a vítima está conectada em M, o servidor que ela hospeda pelo app está fora do ar. O subcaso vale para servidores seeled/VPS ou hospedados por outro processo de versão (ADR 0046), e fica direto no dia em que a multiconexão pedida para a 1.0 existir. Nesse dia M vê o IP público da vítima, e a porta é previsível: 8383, com o UPnP tentando-a primeiro (503b1db). O caso geral, entrar como a vítima em qualquer X que M conheça onde ela foi admitida, vale já.
- **CONFIRMADA**: 3. A fp do link só é comparada depois de o Hello (com convite ou senha) e a resposta assinada terem saído, porque o verificador TLS aceita qualquer certificado no primeiro contato.
  - LIDO: tofu.rs:246-260 (decide fixa o que vier quando não há pin) e :279-281 (FirstContact → ServerCertVerified). TofuVerifier::new só recebe store e pin_key (tofu.rs:227); a fp esperada não entra. client.rs:573-584: o handshake (Hello com join_secret em :1663-1672 e sign em :1700) roda dentro de connect_por. enlace.rs:1265-1291: conferir(&destino, &pin, …) só depois. O teste seele-conformance/tests/convite.rs usa segredo: None (:100) e não prova que o segredo não vazou. **Correção:** Há um agravante que o analista não viu: a corrida de candidatos (enlace.rs:812-870, defasagem de 250 ms) roda o conectar_por completo em cada candidato. Todo candidato que fechar o TLS recebe o Hello com o segredo e a assinatura antes do seu conferir, mesmo quando outro candidato é o vencedor legítimo.
- **CONFIRMADA**: 4. Reconectar pela lista não confere a fp guardada, o endereço devolvido pelo QUEM é aceito como primeiro contato cego, e o quarto pode ser ocupado por quem tem o link quando o anfitrião fica fora mais de 60 s.
  - LIDO: main.rs:981 (impressao_guardada = conhecido.impressao) é usada só em :1007, para perguntar QUEM. :1011-1014 põem a resposta na frente (insert(0)). :1079 expected_fingerprint: esperada.clone() (só do convite). Não há outro ponto em que conhecido.impressao vire expected_fingerprint. camada-servidores.js:150-153 conecta pela lista com null, sem reanalisar link nenhum. Pin por endereço (ffi/lib.rs:3729 chave_do_servidor host:porta; o comentário em ~3790 registra nove portas do mesmo IP no arquivo de pins). tofu.rs:113-128: sem esperada, FirstContact vira Verdict::FirstContact, que é aceito. seele-encontro/src/lib.rs:125-131 (quem chegou primeiro fica, sem chave), :73 (PRAZO 60 s), :281-285 (MORO não tem resposta própria e cai no ONDE). A tela mostra só a faixa PRIMEIRO CONTATO (tela-sessao.js:235-241). **Correção:** Agravante: o endereço do impostor é persistido. O caminhos da reconexão, com o endereço do QUEM já na frente (main.rs:1071), vai para anotar_caminhos (main.rs:1264-1269), que sobrescreve entrada.caminhos (seele-core/src/conhecidos.rs:258-261). O pin do impostor fica gravado sob aquele endereço. Numa visita seguinte com o anfitrião fora do ar, o candidato guardado do impostor dá Matches, vira Known e não mostra faixa nenhuma. É INFERIDO a partir do código lido.
- **CONFIRMADA**: 5. Nenhuma checagem de segurança roda automaticamente: CI só manual, fuzz fora desde 1974d07, corpus não passa do primeiro byte no protocolo 8, deny só no publicar.sh.
  - LIDO: .github/workflows/ci.yml:27-28 e release.yml:21-22 só têm workflow_dispatch. Um grep por deny, audit ou fuzz nos dois workflows não devolve nada. git show 1974d07 remove os jobs cargo-deny, cargo-audit e fuzz smoke (linhas 168-202 do diff). Deny em empacotar/publicar.sh:993-1014. MEDIDO: as 1344 entradas de fuzz/corpus/control_frame começam com 0x01 (699), 0x00 (644) ou 0xbf (1). decode chama negotiate no primeiro byte antes de tudo (control.rs:2321-2329), e com PROTOCOL_VERSION=8 e janela 1 (version.rs:124,164) só 7 e 8 passam. O corpus foi modificado pela última vez em 07/08. MEDIDO: github.com/DATA-AND-DEV/SEELE responde HTTP 200 sem autenticação, ou seja, é público (a API estava com limite estourado). **Correção:** Nuances. O corpus é ignorado pelo git (.gitignore:27, /fuzz/corpus/), então é só local e uma CI começaria sem ele. O libFuzzer acharia o byte 7/8 sozinho, então o corpus velho atrasa mas não inutiliza. O deny no publicar.sh não é garantido: --sem-bateria o pula (publicar.sh:695, :937), e o próprio script registra que a 0.10.1-1 e a 0.10.3 saíram assim (:888). Os alvos de fuzz não cobrem o analisador de datagrama do ponto de encontro (exposto à internet, sem autenticação) nem o de seele://.
- **CONFIRMADA**: 6. O curl … install.sh | sh do README instala a v0.10.0, e não a v0.15.0.
  - LIDO: install.sh:31 REPO="DATA-AND-DEV/SEELE", :72 (releases/latest da API desse repositório), :87 (SEELE_BASE só por variável); install.ps1:28 igual. MEDIDO (a API estava com limite estourado, então fui pelo redirecionamento): `curl -sSI https://github.com/DATA-AND-DEV/SEELE/releases/latest` → location …/tag/v0.10.0; o mesmo em SEELE-RELEASES → v0.15.0. Assets da v0.10.0 (expanded_assets): seele-cli-0.10.0-macos.tar.gz e seele-cli-0.10.0-windows-x86_64.zip, sem Linux. **Correção:** Complemento MEDIDO: a v0.15.0 em SEELE-RELEASES também não tem seele-cli-…-linux. Trocar o REPO conserta macOS e Windows, mas o Linux, que o README promete («macOS e Linux»), continua falhando. No macOS e no Windows o script instala hoje um servidor que nenhum cliente 0.15 alcança (a janela de protocolo é 1).
- **PARCIAL**: [existe] Chave trocada derruba no TLS; limitação de taxa por IP ligada; prévia de link com anti-SSRF.
  - LIDO: tofu.rs:282-285 (Changed → TlsError). session.rs:296 chama portaria.permitir(ip), então o limitador está ligado. Prévia: main.rs:4355-4370 (resolve e recusa endereço reservado), :4397-4398 (Policy::none + resolve_to_addrs, o que fecha o rebinding), com guarda-teste em :8485 e :8653. **Correção:** O limitador pré-autenticação usa HashMap<IpAddr,…> por endereço exato (taxa.rs:220, 233-248), sem agregar IPv6 por /64. Um único /64 enche ENDERECOS_LEMBRADOS=4096 (taxa.rs:71), e a partir daí todo recém-chegado é recusado. O contador recusas_por_lotacao (taxa.rs:269) só é lido num teste (:595): um contador que ninguém lê.
- **CONFIRMADA**: [ausente] Caminho na interface para aceitar troca legítima de chave; esquecer não apaga o pin.
  - LIDO: main.rs:6871-6877 (esquecer só mexe na lista). Um grep por unpin, desfixar ou apagar_pin em apps/seele-app/src e seele-ffi/src/lib.rs não acha nada. frases.js:411-417 pede «Confirme por outro canal» sem oferecer ação nenhuma.
- **CONFIRMADA**: [construido_mas_nao_ligado] Revogação assinada de versões não é consultada pelo app.
  - LIDO: seele-lancador/src/resolucao.rs:196-249 usa Vigente, mas apps/seele-app/src não chama conciliar_revogacoes nem Vigente. As Revogacoes de catalogo.rs:179-308 são de MODs, não de versões do app.

### Severidades contestadas

- **Servidor malicioso se passa por você em outro servidor (relay) e a fp do link é conferida tarde demais**: o revisor propõe *bloqueante_1_0 (mantida), mas separada em duas correções de custos diferentes*. O relay (afirmação 2) exige mudar o que é assinado: um contexto fixo + a fp do certificado do servidor + o nonce, ou um exporter TLS. É protocolo 9, e a 1.0 é o momento mais barato, porque depois dela a compatibilidade passa a cobrar. A conferência tardia (afirmação 3) é só no cliente: passar a fp esperada ao TofuVerifier e recusar no TLS, antes do Hello (tofu.rs:227; client.rs:573). Isso é horas, não dias, e não pede protocolo novo. O exemplo «Comandante do seu próprio servidor» não é explorável hoje pelo app (disconnect derruba a hospedagem, main.rs:2005-2018; sem multiconexão, main.rs:899). Passa a ser explorável quando a multiconexão pedida pelo dono existir, o que é mais um argumento para corrigir antes.
- **«Link que não muda» sem conferência: a reconexão pela lista aceita o impostor do quarto**: o revisor propõe *bloqueante_1_0 (mantida), com custo menor que o estimado*. A gravidade se sustenta e piora: o endereço do impostor é persistido em conhecidos e fixado, e a visita seguinte pode vir sem faixa nenhuma. O conserto, porém, não precisa de MORO assinado nem de protocolo 9. Basta expected_fingerprint = esperada.or(impressao_guardada) em main.rs:1079, somado à conferência no TLS do item anterior. O dado já está lá (main.rs:981) e hoje só serve para perguntar QUEM. Com isso, ocupar o quarto volta a ser só negação de serviço, que é a premissa em que o ADR 0047 §2.4 se apoiou.
- **seeled nasce aberto e o primeiro a entrar vira Comandante**: o revisor propõe *desejavel*. O caminho principal do produto (HOSPEDAR AQUI) semeia a portaria ligada e pré-admite o dono (main.rs:1582-1607). O seeled avisa em voz alta quando está aberto e fora do loopback (seele-server/src/main.rs:109-117), e o ADR 0030 descreve a escolha como deliberada. O risco só existe para quem sobe seeled numa VPS e demora a entrar. Não é o usuário típico da 1.0.
- **Ponto de encontro sem limitação de taxa**: o revisor propõe *importante (com fundamento mais forte que o dado pelo analista)*. Não é só abuso difuso. O teto global MARCAS_NO_QUARTO=4096 (seele-encontro/src/lib.rs:83, :134-138) e a recusa silenciosa deixam qualquer um, com uns 70 datagramas/s de MORO com marcas inventadas (UDP, até com origem forjada), impedir todo anfitrião novo ou que voltou depois de 60 s de se registrar. É a feature de persistência pedida pelo dono desligada para todos, sem que nenhum anfitrião saiba.

### O que o analista não viu

- A decisão de não autenticar o quarto se apoia numa premissa que o código desmente. docs/adr/0047…md:178 diz que o teto do estrago é *não entrar*, porque «quem chega confere a impressão digital de qualquer jeito». seele-proto/src/encontro.rs:65-67 repete isso, e :86-89 afirma que quem recebe o convite «nunca lê resposta nenhuma daqui». Mas main.rs:1006-1030 lê a resposta do QUEM e a põe na frente, e :1079 só confere a fp do convite. A documentação garante uma defesa que a reconexão pela lista não aplica: o produto sabe (impressao_guardada, main.rs:981) e não usa.
- O endereço devolvido pelo QUEM na reconexão é persistido. Ele entra em alternativos (main.rs:1011-1014), vira caminhos (:1071) e é gravado por anotar_caminhos (main.rs:1264-1269; seele-core/src/conhecidos.rs:258-261). Somado ao pin por endereço, um sequestro de 60 s deixa rastro permanente na lista, e uma visita futura com o anfitrião fora do ar pode entrar no impostor como Known, sem faixa nenhuma. É INFERIDO a partir do código lido.
- A corrida de candidatos manda o segredo para todos. correr (enlace.rs:812-870, 250 ms de defasagem) roda o conectar_por completo em cada candidato, com Hello + join_secret + assinatura antes do conferir. O endereço do QUEM sai na posição 0 sem espera. Até a LAN de terceiros entra: o primeiro alternativo do link é o IP privado do anfitrião, e na rede de quem recebe esse IP pode ser outra máquina. Tudo isso recebe o convite de uso único ou a senha reutilizável.
- Com CGNAT, o quarto pode ser tomado mesmo com o anfitrião no ar. seele-encontro/src/lib.rs:125 deixa mudar o registro a qualquer um que venha do mesmo IP público («antigo.ip() == onde.ip()»), e CGNAT é comum em operadoras móveis e de fibra no Brasil. Basta ter a marca, que são 16 hex da fp, visível a qualquer um que já conectou. INFERIDO.
- O caminho legítimo mostra «PRIMEIRO CONTATO — Ninguém confirmou» a cada troca de porta do NAT. O pin é por host:porta (ffi/lib.rs:3729), e o comentário em ~3790 registra nove portas do mesmo IP no arquivo de pins. Isso ensina quem usa a ignorar justamente a faixa que deveria alertar. O mesmo conserto da afirmação 4 (fp guardada como esperada) troca isso por FirstContactVerified.
- O desafio é um oráculo de assinatura. O cliente assina até 256 bytes arbitrários escolhidos pelo servidor (control.rs:273, :2719; client.rs:1700), sem contexto fixo. Hoje a identidade não assina mais nada, mas a feature «calls privadas» pedida pelo dono naturalmente reusaria a identity.key para autenticar chaves de sessão. Sem separação de domínio, qualquer servidor obteria assinaturas sobre mensagens escolhidas. O conserto da afirmação 2 precisa incluir um prefixo de domínio, e o desenho de E2EE (specs/08-seguranca.md:55-65) precisa exigi-lo.
- O limitador pré-autenticação é por IP exato (taxa.rs:220, :233-248), sem agregar IPv6 por /64. Um único /64 enche a tabela de 4096 e passa a recusar todo recém-chegado, e ainda inunda a fila da portaria com chaves novas, que custam nada. O único sinal, recusas_por_lotacao (taxa.rs:269), só é lido num teste (:595). O anfitrião nunca fica sabendo.
- Afirmação 1: File::create (client.rs:2114) também sobrescreve em silêncio um arquivo legítimo de mesmo nome em Downloads, por exemplo dois «foto.png». A confirmação (tela-sessao.js:3306) não avisa. O conserto certo cobre os dois casos: basename + recusa de separadores e .., e create_new com sufixo.
- Linux sem pacote: nem a v0.15.0 publica seele-cli-…-linux (MEDIDO via expanded_assets de SEELE-RELEASES). Corrigir o REPO do install.sh não resolve o Linux, que o README anuncia.
- Pin por endereço sem saída na interface vai além da troca legítima de chave. Dois servidores diferentes no mesmo IP privado e porta, por exemplo 192.168.0.10:8383 na casa de dois amigos, dão «A CHAVE DO SERVIDOR MUDOU» permanente para quem visita os dois. esquecer não desfixa (main.rs:6871-6877), e nenhum comando da interface desfixa.
- Contraponto positivo que o analista não citou: a CSP da janela é estrita (tauri.conf.json: script-src 'self', sem unsafe-inline) e não há innerHTML nem insertAdjacentHTML em apps/seele-app/ui. Isso reduz muito o risco de um texto de chat virar execução na webview que tem acesso ao IPC do Tauri.

### Desenho proposto pelo analista

## Ordem proposta, com o que reusa

### Etapa 0: horas (antes de qualquer outra coisa)
1. **Salvar anexo seguro, decidido no Rust.** `salvar_anexo(anexo)` deixa de receber `destino`. O Rust pega a pasta de `pasta_de_downloads` e o `file_name` do histórico do próprio cliente, como a prévia já faz (`seele-ffi/src/lib.rs:1482-1484`: «The claim the sender made is read from this client's own history»).
   - O nome é reduzido a `Path::file_name()`.
   - São recusados `.`, `..`, separadores, caracteres de controle e bidi, e nomes reservados do Windows (CON, NUL…). O que sobrar vira `anexo-<id>`.
   - A abertura usa `OpenOptions::create_new` com sufixo ` (1)`, e nunca trunca.
   - O caminho final é mostrado de volta na confirmação.
   - Teste de regressão: salvar `../x` cai dentro de Downloads. Reverter o conserto e ver o teste falhar.
   - Corrigir o comentário de `attachment.rs:76-80`.
2. **SECURITY.md** com um contato para reporte.

### Etapa 1: dias, protocolo 9 (já aceito como padrão pela 0.15)
3. **A conferência vai para dentro do TLS.**
   - `TofuVerifier::new(store, pin_key, esperada: Option<String>)`.
   - No primeiro contato com fp esperada divergente, `verify_server_cert` devolve erro, e nenhum byte de aplicação sai.
   - Reusa `verdict`, que continua para `InviteDisagrees`.
   - `conferir` e `desfazer_pin_orfao` ficam como rede de segurança.
4. **A assinatura passa a ter destinatário.**
   - Mensagem assinada: `b"SEELE-auth-v9\0" || fp_do_certificado_do_servidor || nonce(32)`. O cliente tira a fp de `verifier.last_decision()` e recusa nonce com tamanho diferente de 32.
   - O servidor confere com a própria `fingerprint()` (`tls.rs:99`).
   - Se o Tauri ou o quinn expuserem o exporter TLS, preferir `export_keying_material` à fp.
   - Isso fecha o relay e o oráculo de assinatura. É pré-requisito de qualquer «chamada privada» com E2EE.
   - Teste em `seele-conformance`: um servidor-relé tenta entrar em X como V e é recusado. Provar revertendo.
5. **Subir `PROTOCOL_VERSION` para 9**, com nota de release no mesmo tom da 0.15.

### Etapa 2: dias, persistência sem impostor (depende da etapa 1)
6. **A reconexão pela lista passa `conhecido.impressao` como `expected_fingerprint`** (`main.rs:1079`). Com a etapa 1, um endereço vindo do QUEM com outra chave morre no TLS. Considerar fixar o pin pela identidade do servidor quando a fp é conhecida, e não pelo endereço: isso acaba com a faixa PRIMEIRO CONTATO a cada remapeamento de NAT e com o cansaço de alerta.
7. **O ponto de encontro deixa de ser «quem chega primeiro».**
   - Mínimo (P): o anfitrião pergunta QUEM pela própria marca depois de cada MORO. Se a resposta for outro IP, avisa na tela: «alguém ocupou o seu lugar no ponto de encontro». Reusa `onde_mora`.
   - Certo (M): `SEELE-ENC/2`, com MORO assinado pela chave do certificado do servidor e a marca derivada dessa chave. O encontro confere a assinatura, e o anfitrião legítimo desaloja o impostor.
   - Balde por origem, por /64 em IPv6, reusando o desenho de `taxa.rs`.

### Etapa 3: horas, voltar a medir sozinho (repositório público, minutos grátis)
8. **`ci.yml` com `push` e `pull_request`**, pelo menos em Linux, mais um job `cargo deny check` e um agendado semanal só de `advisories`.
9. **Fuzz**:
   - Regenerar o corpus com o byte 8.
   - Criar alvos novos para `uri::analisar`, `encontro::analisar`, `AttachmentHeader`, `read_manifest` e os cabeçalhos de `stream` e `screen`.
   - Rodar um smoke de 60 s por alvo no CI, que é o job apagado em 1974d07, trazido de volta.

### Etapa 4: dias a semanas, distribuição
10. **Scripts de instalação**: `install.sh` e `install.ps1` passam a usar `DATA-AND-DEV/SEELE-RELEASES`, e o segundo endpoint do atualizador sai. Publicar o Linux (`.deb` e o tar da CLI) ou tirar o Linux do README.
11. **Assinar o `SHA256SUMS`** com minisign, com uma chave própria ou a do atualizador, conforme o critério do ADR 0045 de uma chave por afirmação.
12. **Revogação ligada**: `empacotar/manifesto.py` emite o envelope assinado que `seele-lancador` já lê. `versoes.rs` passa a resolver pelo `seele_lancador` em vez de montar `VersaoResolvida` à mão. Depois das etapas 0 a 2, revogar as versões ≤0.15 que tenham o defeito do anexo.
13. **Chave do atualizador de reserva**, guardada offline.
    - INFERIDO: o plugin do Tauri aceita uma `pubkey` só. A reserva então entra na conferência do `seele-lancador`, que é código próprio e pode aceitar duas.
    - Decidir isto **antes** da 1.0: depois, a base instalada é que paga.
14. **Assinatura de SO**: Apple Developer ID com notarização, e Azure Artifact Signing. Os ganchos estão em `release.yml:277-393` e só faltam os segredos. Ligar `hardenedRuntime` junto; o `Entitlements.plist` do microfone já está pronto. Tirar o `xattr -dr` do README.

### Etapa 5: horas, padrões e superfície
15. **`seeled` nasce com a portaria ligada**, reusando `semear_ligada`. Ou então o Comandante só é concedido a quem apresentar um token impresso no terminal na primeira subida.
16. **Portaria mais resistente**:
    - O balde passa a usar a chave `/64` em IPv6.
    - Teto de pedidos pendentes (por exemplo, 200) descartando o mais antigo, e limpeza depois de N dias.
    - Notificação do sistema.
17. **Recuperar de troca de chave**: um link novo cuja fp **é** a que o servidor oferece autoriza substituir o pin, porque essa é a prova. Sem link, um botão exige colar ou digitar a fp nova.
18. **Honestidade de privacidade**:
    - README e tela de hospedar dizem «quem hospeda vê todo o histórico e pode gravar o áudio da sala».
    - Uma página de privacidade para `encontro.seele.app.br` e `mods.seele.app.br` diz o que se aprende e por quanto tempo.
    - A `specs/08` é corrigida: hoje o produto fala com o encontro, o catálogo, o GitHub e a Cisco.

### Relação com as features pedidas
- **Persistência e URL amigável**: só é segura depois das etapas 1 e 2, porque o caminho que ela cria, a lista mais o QUEM, é exatamente o que hoje aceita impostor. No link com nome, o pin pelo nome ajuda contra sequestro de DNS depois do primeiro contato; o primeiro contato depende da etapa 1.
- **Multiconexão**: não muda o modelo de ameaça, mas multiplica as sessões que usam a mesma identidade. O relay fica mais valioso, então a etapa 1 vem antes.
- **Chamadas privadas**: se «privada» for em relação aos outros membros, é permissão de sala. Se for em relação a quem hospeda, é E2EE, e exige a etapa 1 (assinatura com domínio e vínculo) mais o esboço de `specs/08-seguranca.md:59-63`. Não dá para prometer isso na 1.0 sem esse trabalho.

### Perguntas ao dono

- Você aceita que a 1.0 suba o protocolo para 9, e que a 0.15 e a 1.0 não conversem, para amarrar a assinatura ao servidor e mover a conferência da fp para dentro do TLS? É a correção de maior efeito, e fica mais barata agora do que depois da 1.0.
- «Chamadas privadas» são privadas em relação a quem: aos outros membros do servidor (uma sala com permissão) ou também a quem hospeda (E2EE)? A segunda é um projeto de semanas e não cabe como promessa na 1.0.
- Há orçamento para o Apple Developer ID (US$ 99/ano) e para o Azure Artifact Signing (cerca de US$ 10/mês) antes da 1.0? Os ganchos no release.yml já existem.
- Onde está hoje a chave privada do atualizador (~/.tauri/seele.key) e a senha dela? Existe cópia offline? Posso propor uma segunda chave pública de reserva antes da 1.0?
- O encontro.seele.app.br e o mods.seele.app.br são operados por você, em nome de quem? Topa publicar uma página de privacidade (LGPD) dizendo o que o ponto de encontro aprende (IPs, servidores vivos, quem procura quem)?
- O seeled pode nascer com a portaria ligada (ou exigir um token de primeira subida para virar Comandante) na 1.0, mesmo mudando o comportamento para quem já opera um?
- O repositório DATA-AND-DEV/SEELE está público de propósito? Se estiver, os minutos de Actions são grátis e a CI automática (com deny e fuzz) pode voltar hoje; se não, é preciso rever a visibilidade (também está sem licença).
- A v0.15.0 não tem pacote Linux e o README promete .deb: o Linux entra na 1.0 ou sai do README?

---

## Distribuição, versões e compatibilidade

**Resumo do analista.** O SEELE publica só dois alvos: macOS Apple Silicon e Windows x64. Nenhum dos dois tem assinatura do sistema operacional, e os pacotes são montados à mão nas máquinas do dono. O workflow de Release nunca passou (0 de 69 execuções) e o CI está vermelho desde 16/08. O atualizador minisign funciona, mas a compatibilidade entre versões não existe na prática. Todo quadro de controle sai carimbado com a versão global do protocolo, e o cabeçalho de voz exige a mesma versão exata. Por isso cada subida de protocolo (três em 17 dias) derruba o grupo inteiro, e as duas pontas leem frases erradas: «NADA RESPONDEU NESSE ENDEREÇO» ou «PROTOCOLO VIOLADO». A saída que as notas da 0.15 prometem, guardar a 0.14.2 ao lado, não funciona com o manifesto publicado. E o instalador de uma linha do README instala o seeled 0.10.0, que fala o protocolo 2. A quebra de 7 para 8 era evitável: as listas de mensagens só cresceram por acréscimo no fim, e o servidor já deixa de mandar o que o par não entende. O mínimo para a 1.0 nesta frente tem quatro partes: uma janela de protocolo real, com recusa que qualquer versão consiga ler; a notarização no macOS; os instaladores de uma linha corrigidos; e a decisão de licença. O launcher do ADR 0046 deve sair do caminho crítico, porque é incompatível com a conexão simultânea.

### O que existe

- `construido_e_publicado`: Atualizador assinado (minisign, tauri-plugin-updater), com conferência antes de tocar no disco (apps/seele-app/src/main.rs:7395-7600; tauri.conf.json plugins.updater; latest.json + .sig publicados)
- `parcial`: Negociação de versão de protocolo (janela N−1) (crates/seele-proto/src/version.rs:124-235; control.rs:2298-2334)
- `construido_e_publicado`: O servidor deixa de mandar variantes e motivos novos para pares de versão anterior (crates/seele-server/src/session.rs:4177-4230 (entende_a_mensagem), teste em :5138)
- `parcial`: Frase de versão incompatível na interface (apps/seele-app/ui/frases.js:32-34)
- `parcial`: Versão do produto no link seele:// (v=) (crates/seele-proto/src/uri.rs:394,472; main.rs:6918-6950)
- `construido_mas_nao_ligado`: Launcher de versões lado a lado (depósito, dados por versão, revogação) (crates/seele-lancador (5.018 linhas); apps/seele-app/src/versoes.rs; ui/camada-versoes.js)
- `so_desenho`: Manifesto v2 (schema, versions[], executable, sha256, unit, revocations) (docs/versoes-lado-a-lado.md §2-3; empacotar/manifesto.py não o gera)
- `construido_e_publicado`: Cabeçalho de tela com versão própria, desacoplada do protocolo (crates/seele-proto/src/screen.rs:78)
- `ausente`: Cabeçalho de voz com versão própria (crates/seele-proto/src/media.rs:185 (igualdade estrita com PROTOCOL_VERSION))
- `construido_e_publicado`: Empacotamento e publicação manuais (macOS, Windows por SSH, Linux em Docker) com a bateria de testes (empacotar/publicar.sh, macos.sh, windows.ps1, linux.sh, manifesto.py)
- `construido_mas_nao_ligado`: Workflow de Release no GitHub Actions (.github/workflows/release.yml (0 de 69 execuções verdes))
- `parcial`: CI nos três sistemas (.github/workflows/ci.yml (só workflow_dispatch; verde pela última vez em 16/08))
- `construido_mas_nao_ligado`: Pacote .deb para Linux (empacotar/linux.sh; artefato entrega-linux do CI da v0.14.2, não publicado)
- `parcial`: macOS universal (Intel + Apple Silicon) (release.yml:247-251 (job que falha); macos.sh só gera a arquitetura da máquina)
- `parcial`: Instaladores de uma linha com conferência de SHA256 (install.sh:31, install.ps1:28 (apontam para o repositório errado → v0.10.0))
- `parcial`: Assinatura de sistema (Apple Developer ID e notarização; Authenticode) (release.yml monta tudo se houver os segredos; tauri.conf.json signingIdentity "-"; plano Azure inelegível no Brasil)
- `ausente`: Guarda contra abrir um banco migrado por versão mais nova (crates/seele-server/src/persistence/mod.rs:272-301)
- `construido_mas_nao_ligado`: Checagem de licenças das dependências (cargo deny) (deny.toml (passa localmente; não roda no CI))
- `ausente`: Licença do projeto e avisos de terceiros dentro do pacote (README.md:322; bundle .app sem arquivo de licença)
- `parcial`: Condições de uso do binário OpenH264 da Cisco (aviso, controle, reprodução da licença) (crates/seele-video/src/modulo.rs:41; index.html:3959-3966)
- `ausente`: seeled no PATH do Windows (docs/pendencias.md:263-275; apps/seele-instalador)

### Lacunas

- **[BLOQUEANTE · esforço G · risco medio] Subir o protocolo derruba o grupo inteiro, porque não existe janela real.** A janela N−1 só vale para quem ouve: `encode` carimba a versão global em todo quadro, e o `decode` do par mais velho recusa esse carimbo antes de ler o corpo. Resultado: toda subida de protocolo (3 em 17 dias: 3→6, 6→7, 7→8) é uma atualização obrigatória e simultânea para o grupo inteiro, com quem hospeda incluído. Para a conexão simultânea e para servidores persistentes (duas features pedidas pelo dono), isso é fatal: um servidor que ficou um dia atrás tranca todo mundo que atualizou. As listas de mensagens são só acrescidas no fim desde a 0.11, e o servidor já deixa de mandar o que o par não entende. O que falta é carimbar cada quadro com a versão negociada e aceitar versões numa janela.
  - *Evidência:* control.rs:2301 (frame.push(PROTOCOL_VERSION)); control.rs:2326; version.rs:164,221-235; pendencias.md:6313-6360 (#42); listas de ClientMessage, ServerMessage e DisconnectReason medidas por `git show v0.11.0|v0.14.2|HEAD:…control.rs`; session.rs:4177-4230
- **[BLOQUEANTE · esforço P · risco medio] Voz entre versões diferentes é descartada em silêncio.** O cabeçalho do datagrama de voz exige `version == PROTOCOL_VERSION` exato, embora o layout de 11 bytes não mude desde o primeiro commit. Assim que a janela de controle funcionar, a voz entre um par da versão N−1 e um da versão N cai inteira: o servidor conta como `malformed` e o cliente faz `continue`, e nenhuma das pontas diz nada. É o mesmo defeito que já levou ao desacoplamento do `SCREEN_HEADER_VERSION`. O servidor já cria uma cópia do datagrama por assinante, então dá para reescrever o byte 0 por destinatário sem custo extra.
  - *Evidência:* media.rs:185 e 233; voice_room.rs:1057 (drops.malformed) e :1116 (bytes.to_vec() por assinante); voice.rs:1914; screen.rs:78 como precedente
- **[BLOQUEANTE · esforço M · risco baixo] Recusa por versão aparece como falha de rede ou «PROTOCOLO VIOLADO».** Cliente 0.14 num host 0.15: a tela diz «NADA RESPONDEU NESSE ENDEREÇO» e manda a pessoa conferir porta e firewall. Cliente 0.15 num host 0.14: a tela diz «PROTOCOLO VIOLADO». A frase certa existe e é inalcançável. O produto sabe a versão (Hello.client carrega a versão do produto; o link carrega v=) e não conta. Os ordinais de `Disconnecting` e `DisconnectReason::Incompatible` são os mesmos de 0.11.0 até o HEAD. Então um servidor 1.0 que carimbe a recusa com o byte que o par anunciou faz até um cliente 0.11–0.15 ler «VERSÃO INCOMPATÍVEL», sem mudar nada no cliente velho.
  - *Evidência:* session.rs:467-472 (Hello ilegível → ProtocolViolation), :488-491 (ramo Incompatible inalcançável), :333-346; client.rs:1678-1681 (→ Unreachable); frases.js:32-34, 47, 543, 598; camada-servidores.js:187-197 (conecta sem avisar quando a versão do link não está instalada)
- **[BLOQUEANTE · esforço P · risco baixo] Os instaladores de uma linha do README instalam o seeled 0.10.0 (protocolo 2).** `install.sh` e `install.ps1` resolvem `releases/latest` no repositório de código, e o último release lá é a v0.10.0. Quem segue o README para ter um servidor sempre ligado (justamente o caso do servidor persistente) ganha um `seeled` com o qual nenhum cliente atual conversa, e o cliente dirá «NADA RESPONDEU». No Linux o script dá 404. O segundo endpoint do atualizador aponta para o mesmo lugar: se o primeiro falhar, o app responderia «VOCÊ ESTÁ NA ÚLTIMA VERSÃO».
  - *Evidência:* install.sh:31, install.ps1:28; MEDIDO: https://github.com/DATA-AND-DEV/SEELE/releases/latest → /tag/v0.10.0; `git show v0.10.0:crates/seele-proto/src/version.rs` → PROTOCOL_VERSION = 2; …/SEELE/releases/latest/download/latest.json diz 0.10.0; tauri.conf.json plugins.updater.endpoints[1]
- **[BLOQUEANTE · esforço M · risco medio] O app para macOS não é notarizado: o primeiro contato é «Mover para o Lixo».** A assinatura é ad-hoc e o `spctl` rejeita o app. No macOS atual, o atalho do botão direito que o README ensina não é mais o caminho documentado pela Apple: é Ajustes → Privacidade e Segurança → «Abrir Mesmo Assim», mais a senha. Um amigo não técnico não chega lá sozinho. Resolver custa o Apple Developer Program (US$ 99/ano), `hardenedRuntime: true`, assinar o `seeled` embutido e rodar o `notarytool` em `macos.sh`. Há um risco concreto de regressão: com hardened runtime, a validação de bibliotecas bloqueia o `dlopen` do libopenh264 assinado pela Cisco, então é preciso o entitlement `com.apple.security.cs.disable-library-validation`, e isso tem de ser testado. Também convém medir se, com a assinatura ad-hoc de hoje, cada atualização faz o macOS pedir de novo a permissão de microfone e de gravação de tela (INFERIDO, não medido).
  - *Evidência:* MEDIDO: codesign -dv → Signature=adhoc, TeamIdentifier=not set; spctl -a → rejected; tauri.conf.json bundle.macOS {signingIdentity "-", hardenedRuntime false}; README.md:124-130; support.apple.com/102445 (consultado em 22/09) documenta só «Open Anyway» em Ajustes
- **[importante · esforço M · risco baixo] Windows sem Authenticode, e o plano de assinatura (Azure) não atende o Brasil.** Sem assinatura, o SmartScreen avisa e o Smart App Control bloqueia sem opção de contornar. O `release.yml` recusa o job do Windows sem os segredos do Azure. Pela documentação atual da Microsoft, porém, o Artifact Signing de confiança pública só atende organizações de 12 países/regiões (o Brasil não é um deles) e pessoas físicas nos EUA e no Canadá. A alternativa é um certificado OV de uma CA com assinatura em nuvem (custo anual maior). Mesmo assinado, o SmartScreen só se cala depois de a reputação se formar. Não marquei como bloqueante porque o caminho «Mais informações → Executar assim mesmo» funciona para a maioria.
  - *Evidência:* MEDIDO: tabela de certificados do PE com 0 bytes; anotação do job windows da v0.14.2 («Falta: AZURE_ENDPOINT…»); learn.microsoft.com/azure/artifact-signing/quickstart (Prerequisites, atualizado em 18/09/2026); docs/assinatura-e-atualizacao.md (SAC bloqueia); pendencias.md:2405-2427
- **[importante · esforço P · risco baixo] «Guardar a versão anterior ao lado» é anunciado e não funciona.** É a saída que as notas da 0.15 oferecem para a quebra de protocolo. O botão só baixa a mais nova, e a instalação falha com `ExecutavelNaoDeclarado` porque o manifesto publicado não traz `executable`. No Windows o alvo é um `.exe`, que não se abre ao lado de outro. A documentação ainda manda publicar o caminho de executável errado. Enquanto o manifesto v2 não existir, o seletor «VERSÃO QUE VAI HOSPEDAR» tem uma opção só.
  - *Evidência:* versoes.rs:326-372 (baixar_a_mais_nova), :283-297; deposito.rs:276-279; manifesto.rs:182-188; latest.json v0.15.0 MEDIDO sem executable/sha256/unit; empacotar/notas/0.15.0.md:11-14; index.html:534-537; docs/versoes-lado-a-lado.md:253 vs CFBundleExecutable=seele-app (MEDIDO)
- **[importante · esforço P · risco baixo] A publicação não enxerga o protocolo, e as notas automáticas mentem.** Nada no `publicar.sh` nem no `release.yml` compara `PROTOCOL_VERSION` e `MIGRATIONS` com os da última release. O classificador de notas depende de prefixos `feat:`/`fix:`, que 247 de 303 commits não usam. A página da v0.15.0 afirma «não traz nenhuma mudança de produto» logo abaixo do aviso de que ela não conversa com a v0.14. Dentro do app, a tela de atualização mostra só um link como nota.
  - *Evidência:* publicar.sh:519-593; grep vazio de PROTOCOL_VERSION em publicar.sh e release.yml; corpo do release v0.15.0 (MEDIDO); `git diff --stat v0.14.2 v0.15.0` = 91 arquivos, +16.540; manifesto.py:113; tela-server.js:1279-1287
- **[importante · esforço P · risco baixo] Um build velho abre em silêncio o banco migrado por um build novo.** `migrate()` não confere se `schema_version` passou do que o build conhece. Voltar de versão reinstalando o `.dmg` ou o `.exe` antigo (o caminho que sobra, já que o launcher não instala) roda código velho contra um esquema novo sem dizer nada. Hoje é seguro por acaso: a 0.15 não trouxe migração.
  - *Evidência:* persistence/mod.rs:272-301; contagem de migrações por tag (13 desde a v0.12.0, MEDIDO); main.rs:848-864 (mesmo diretório para todas as versões)
- **[importante · esforço M · risco baixo] CI vermelho desde 16/08, Release 0 de 69, e publicação dependente de uma pessoa.** Nenhuma verificação pública roda sozinha: os dois workflows são manuais desde 6f970c9, e reprovam. A release sai do Mac do dono e de uma máquina Windows por SSH, sem atestado de procedência. A bateria roda no processo manual, mas o que está escrito sobre CI em pendencias.md (#38, «nunca rodou») não corresponde à realidade. Para uma 1.0, ou o CI volta a ser verde e obrigatório, ou o processo manual passa a ser o oficial, com registro de toolchain e commit (esse registro já existe nas notas).
  - *Evidência:* MEDIDO: API Actions (40 execuções recentes em failure; jobs test linux/macos/windows e clippy macos/windows); página de Actions: último CI verde #28 em 2026-08-16; release.yml ?query=is:success → 0 resultados; ci.yml:27-28; release.yml:21-22; pendencias.md:37
- **[importante · esforço P · risco baixo] Licença indefinida, avisos de terceiros ausentes e condições da Cisco não cumpridas.** Sem licença, o código público é «todos os direitos reservados». Um MOD de terceiros que parte dos exemplos, uma contribuição externa e o indexador que redistribui MODs ficam sem base, e relicenciar fica mais caro a cada contribuição aceita. Independentemente da escolha, redistribuir binários com dependências MIT, Apache e BSD exige reproduzir os avisos, e o pacote não traz nenhum. O uso do binário OpenH264 exige, pela licença da Cisco, a frase de atribuição, um controle para ligar e desligar, e a reprodução da licença num EULA, que não existe. As dependências não restringem a escolha: só licenças permissivas e MPL-2.0; a GPL-2.0-only é a única opção fechada. Não bloqueia quem usa o app, mas é uma decisão de horas que fica mais cara depois.
  - *Evidência:* README.md:322; API do GitHub license=null; `cargo deny --offline check licenses` → licenses ok (MEDIDO) e `cargo deny list -l license` (6 MPL-2.0, 12 crates próprios Unlicensed); listagem do .app sem arquivo de licença; modulo.rs:41; grep sem «provided by Cisco»; openh264.org/BINARY_LICENSE.txt (condições 2-4); api/v*.json sem campo de licença
- **[importante · esforço P · risco baixo] README e modelo de notas anunciam o que não existe.** Anunciam o binário `connection` (e `cargo build --bin connection` falha), «três programas» no pacote, `.deb` no Linux, o atalho do botão direito no macOS e «1.362 testes». Para uma 1.0 pública, a página que a pessoa lê primeiro não pode mandar compilar um alvo que não existe. O roteiro da validação que falta (docs/teste-duas-maquinas.md) também usa o `connection`.
  - *Evidência:* README.md:120,128,159,164,168,171,188,220,270; .github/NOTAS-DE-RELEASE.md:18-28; conteúdo do seele-cli (só seeled, MEDIDO); Cargo.toml [[bin]] = seeled, seele-encontro, seele-app; docs/teste-duas-maquinas.md:255,262,299
- **[importante · esforço M · risco baixo] Mac Intel sem pacote.** Um amigo com Mac Intel simplesmente não entra no grupo: o `.dmg` Apple Silicon não abre lá. A receita universal (lipo) existe no workflow, que falha, e o `macos.sh` gera só a arquitetura da máquina. O mínimo é declarar a plataforma sem suporte na página e no README. O ideal é gerar o universal no `macos.sh`, dobrando o tempo de build no Mac.
  - *Evidência:* macos.sh:5-14; manifesto.py:47-61; release.yml:247-251; lipo -archs = arm64 (MEDIDO)
- **[importante · esforço P · risco baixo] Chave do atualizador em custódia única, segredo do CI pela metade.** Perder a chave privada ou a senha tranca a atualização de todos os instalados. O documento orienta duas cópias, mas só o dono sabe se elas existem. No GitHub só metade do segredo está cadastrada, e por isso nenhum build de CI sai atualizável.
  - *Evidência:* docs/assinatura-e-atualizacao.md:150-160; anotação do job macos da v0.14.2: «só metade da chave do atualizador existe; este release não atualiza ninguém»
- **[desejável · esforço P · risco baixo] Linux sem pacote publicado.** O `.deb` é construído com sucesso no CI (job linux da v0.14.2) e nunca é publicado. No Linux não há compartilhamento de tela, por decisão, e o glib 0.18 tem problema de unsoundness conhecido (#48). Dá para publicar como «Linux: voz e texto, sem tela», ou declarar Linux fora da 1.0.
  - *Evidência:* artefato entrega-linux de 27 MB na execução 35657858980; captura/mod.rs:36-40; pendencias.md:7193-7240
- **[desejável · esforço P · risco medio] seeled fora do PATH no Windows; Windows ARM só por emulação.** Com o `connection` removido, o PATH só importa para o `seeled`, que o app já dispensa (HOSPEDAR AQUI). O Windows ARM roda o x64 por emulação (INFERIDO), sem pacote nativo.
  - *Evidência:* pendencias.md:263-275; grep sem PATH em apps/seele-instalador/src; assets só x64
- **[desejável · esforço P · risco baixo] Documentação interna de versão que aponta para o lugar errado.** `version.rs:97-104` descreve a v7 como «campo novo na Session», mas foi a variante `Instancia`. O teste `a_release_publicada_recusa_o_carimbo_desta_build` fixa `PUBLICADA = 3` e passa por vácuo. O ADR 0031 (multiconexão) e o ADR 0046 (um processo por versão) se contradizem sem se citar.
  - *Evidência:* version.rs:97-104 vs session.rs:4202-4208 e campos de Session idênticos entre v0.11.2 e HEAD (MEDIDO); control.rs:3059-3073; docs/adr/0031 e 0046

### O que o revisor conferiu

- **CONFIRMADA**: 1. Toda subida de PROTOCOL_VERSION derruba as versões vizinhas nos dois sentidos, e as duas pontas veem «NADA RESPONDEU NESSE ENDEREÇO» ou «PROTOCOLO VIOLADO» em vez de «versão incompatível».
  - LIDO, sem teste em execução. Segui os dois sentidos no código das duas versões.

Cliente 0.14 → host 0.15:
- v0.14.2 control.rs:2213 carimba o byte 7.
- O decode da 0.15 (control.rs:2326) chama negotiate(7) com PROTOCOL_VERSION=8 e COMPATIBILITY_WINDOW=1 (version.rs:124,166,221-235), e o 7 passa.
- O Hello (session.rs:467-491) é aceito.
- O Challenge sai carimbado 8. O cliente 0.14 recusa em negotiate(8) (PeerTooNew). O `map_err` de v0.14.2 client.rs:1680-1683 transforma isso em ConnectError::Unreachable, que aparece como «NADA RESPONDEU NESSE ENDEREÇO» (v0.14.2 frases.js:588).

Cliente 0.15 → host 0.14:
- O Hello sai carimbado 8. O servidor 0.14 recusa ainda no decode, e v0.14.2 session.rs:467-472 transforma isso em ProtocolViolation.
- O Disconnecting sai carimbado 7, e o 7 está dentro da janela da 0.15.
- O cliente lê Refused{ProtocolViolation}, que aparece como «PROTOCOLO VIOLADO» (frases.js:47).

Os ordinais de ServerMessage e DisconnectReason são os mesmos nas duas versões. Medi com um script que compara variantes e campos entre v0.14.2 e HEAD em todos os arquivos de crates/seele-proto/src: só houve acréscimos no fim.

Nenhum código do cliente trata ControlError::UnsupportedVersion: o grep só o encontra na definição e em teste (control.rs:333, 2326, 3082). **Correção:** Um exagero de detalhe. A frase de Unreachable é só «NADA RESPONDEU NESSE ENDEREÇO» (frases.js:598); ela não manda conferir porta e firewall. Quem manda é a SemResposta (frases.js:~620).

Um reforço que o analista não disse: o ramo Incompatible do handshake (session.rs:488-491) é inalcançável para qualquer par real, não só para 0.14↔0.15. Um cliente de verdade carimba o mesmo número que põe em Hello.version, então qualquer diferença de versão morre antes, no decode.

A frequência também está subestimada. Foram 4 quebras publicadas:
- 2→3: v0.10.5-1, 05/09;
- 3→6: v0.11.0, 18/09;
- 6→7: v0.12.0, 20/09 UTC;
- 7→8: v0.15.0, 23/09 UTC.
Três delas caíram em cerca de 4,5 dias. MEDIDO com `git show <tag>:crates/seele-proto/src/version.rs` e com a data de cada página de release em SEELE-RELEASES.
- **PARCIAL**: 2. A quebra de 7 para 8 era evitável: o vocabulário só cresceu por acréscimo no fim, o servidor já cala o que o par não entende, e carimbo negociado mais cabeçalho de voz próprio bastariam para uma janela real.
  - MEDIDO. Entre v0.14.2 e HEAD, só control.rs e version.rs mudaram em seele-proto (`git diff --stat`). O script de comparação dá:
- AlertReason: de 15 para 16 variantes, a nova no fim;
- ServerMessage: de 40 para 41, a nova no fim;
- MessageRefusal: tipo novo.

Entre v0.11.0 e v0.14.2, ServerMessage foi de 38 para 40, as duas no fim. Houve uma inserção no meio, mods::Refused (ApiTooOld no índice 3), mas esse tipo não deriva Serialize (mods.rs:176) e não vai pelo fio.

LIDO. O portão entende_a_mensagem (session.rs:4177-4230) cobre MessageRejected e AlertReason::RetencaoApagouHistorico, e os três pontos de envio passam por ele (session.rs:919, 2098, 3520).

LIDO, e isto derruba a segunda metade. negotiate() recusa com PeerTooNew qualquer par mais NOVO (version.rs:221-226). O cliente manda sempre Hello{version: PROTOCOL_VERSION} (client.rs:1667). E nada em seele-core aprende ou se adapta à versão do servidor: o grep por 'protocol_version|negotiate' em crates/seele-core/src não acha nada. **Correção:** A primeira metade vale para um sentido só, cliente velho → servidor novo. Um servidor 0.15 que carimbasse 7 para quem falou 7, e aceitasse o cabeçalho de voz 7, teria recebido os clientes 0.14.

O outro sentido, cliente novo → servidor velho, não se resolve com carimbo negociado. É justamente o caso que o analista usa para servidores persistentes: «um servidor que ficou um dia atrás tranca todo mundo que atualizou». Mesmo com os dois lados carimbando pela versão negociada, um cliente N+1 manda Hello.version=N+1, e o negotiate do servidor N recusa com PeerTooNew.

Uma janela de verdade nos dois sentidos pede mais uma peça: o cliente descer de versão. Pode ser anunciar uma faixa mín–máx no Hello e o servidor escolher, ou tentar de novo em N−1 depois de uma recusa. O Hello teria de ser carimbado com a versão mais velha que o cliente suporta. É mudança de handshake, maior que «carimbo + cabeçalho de voz».
- **CONFIRMADA**: 3. A saída anunciada para a quebra da 0.15 (guardar a v0.14.2 ao lado e escolher a versão que hospeda) não funciona com o manifesto publicado.
  - MEDIDO. `curl -sSL https://github.com/DATA-AND-DEV/SEELE-RELEASES/releases/latest/download/latest.json` devolve version 0.15.0 e só darwin-aarch64 e windows-x86_64, cada um com url e signature. Não há `executable` nem `versions[]`.

LIDO. versoes.rs:321-372: `baixar_a_mais_nova` pega sempre `manifesto.mais_nova()`. Não existe caminho no código para baixar uma versão anterior.

LIDO. deposito.rs:271-279: `entrada.executavel()` → ExecutavelNaoDeclarado, antes de conferir qualquer coisa. O próprio teste deposito.rs:762-781 prende que «o manifesto publicado hoje, sem executable» não instala.

MEDIDO. As notas empacotar/notas/0.15.0.md:11-14 prometem «Em CONFIGURAÇÕES dá para baixar a v0.14.2». **Correção:** Duas correções sobre o que já existe.

Primeira: o launcher não está «construído mas não ligado». Ele está ligado na interface:
- index.html:528-541: o seletor VERSÃO QUE VAI HOSPEDAR e a nota «Em CONFIGURAÇÕES dá para baixar outra»;
- camada-servidores.js:186-192;
- main.rs:5531 e 6926-6932.
Só é inoperante com o manifesto publicado. É pior que não estar ligado: a própria tela promete uma saída que falha.

Segunda: mesmo com `executable` no manifesto, a 0.14.2 continuaria inalcançável, porque o botão só baixa a mais nova.
- **CONFIRMADA**: 4. O instalador de uma linha do README instala o seeled 0.10.0, que fala o protocolo 2 e não conversa com nenhum cliente atual.
  - LIDO: install.sh:31 e install.ps1:28 usam REPO=DATA-AND-DEV/SEELE, e install.sh:71 resolve `api.github.com/repos/$REPO/releases/latest`.

MEDIDO:
- github.com/DATA-AND-DEV/SEELE/releases/latest → 302 para /tag/v0.10.0.
- raw.githubusercontent.com/.../main/install.sh → 200, idêntico ao local.
- seele-cli-0.10.0-macos.tar.gz → 200; seele-cli-0.10.0-windows-x86_64.zip existe e é o nome que install.ps1:62 espera; seele-cli-0.10.0-linux.tar.gz → 404.
- `git show v0.10.0:crates/seele-proto/src/version.rs` → PROTOCOL_VERSION = 2.

MEDIDO: o release v0.15.0 em SEELE-RELEASES publica seele-cli-0.15.0-macos.tar.gz e seele-cli-0.15.0-windows-x86_64.zip. Trocar REPO basta para macOS e Windows. **Correção:** Um atenuante para o app gráfico. A v0.10.0 do repositório de código tem os mesmos endpoints, SEELE-RELEASES primeiro, e a mesma chave pública do atualizador (MEDIDO: `git show v0.10.0:apps/seele-app/tauri.conf.json`). Quem baixa o app pela aba Releases do repositório de código é atualizado sozinho para a 0.15.0.

O seeled não tem esse socorro: ele não tem atualizador nenhum. Nada em crates/seele-server/src busca latest.json ou atualiza.
- **CONFIRMADA**: 5. Os binários publicados não têm assinatura de sistema, e o plano de assinatura no Windows (Azure Artifact Signing) não é executável a partir do Brasil.
  - MEDIDO na cópia instalada aqui, que é a 0.14.2 (CFBundleShortVersionString):
- `codesign -dv /Applications/SEELE.app` → Signature=adhoc, TeamIdentifier=not set;
- `spctl -a` → rejected;
- `codesign -d -r-` → designated => cdhash H"d2e2…".

LIDO:
- tauri.conf.json bundle.macOS: signingIdentity "-", hardenedRuntime false;
- empacotar/macos.sh não tem codesign nem notarytool;
- empacotar/windows.ps1 não tem signtool.

MEDIDO pelo WebFetch da quickstart da Microsoft (updated_at 2026-09-18): Public Trust só para organizações de US, CA, UE, UK, AU, NZ, JP, KR, SG, CH, NO e IL, e pessoa física só em US e CA. O Brasil ficou de fora, embora a região Brazil South exista para o serviço.

Não medi a tabela de certificados do PE do .exe, para não baixar o executável. **Correção:** A conclusão prática não é «sem assinatura no Windows». Só o plano Azure fica fora. Certificados OV tradicionais de CA (em token de hardware ou HSM na nuvem) continuam acessíveis a quem está no Brasil (INFERIDO, não conferi preço nem disponibilidade). O release.yml teria de trocar o passo Azure por um signtool com esse certificado.
- **CONFIRMADA**: 6. Nenhuma verificação automática e pública protege o que é publicado: o CI está vermelho desde 16/08 e o workflow de Release nunca passou.
  - MEDIDO pelo HTML público das Actions, porque a API estourou o limite de requisições.

- release.yml: «69 workflow runs»; `?query=is:success` → «0 workflow run results». A execução 69 (21/09) falhou.
- ci.yml: 275 execuções; a última verde é de 2026-08-16T02:57:49Z; a 275 (22/09) falhou.
- Na execução 35762838943 falharam 5 jobs: clippy (macos) com exit 101, clippy (windows) com 1, test (linux) e test (macos) com 101, test (windows) com 1. São falhas de verdade de clippy e de teste nos três sistemas, não de infraestrutura.
- O Release 35657858980 anota «Falta: AZURE_ENDPOINT, AZURE_ARTIFACT_SIGNING_ACCOUNT, AZURE_ARTIFACT_SIGNING_CERTIFICATE_PROFILE» e «só metade da chave do atualizador existe».

LIDO: ci.yml:27-28 e release.yml:21-22 só têm workflow_dispatch. **Correção:** Um reforço: o Release não falha por instabilidade. Ele exige segredos Azure que, pela afirmação 5, não se obtêm do Brasil (release.yml:277-280 e 318-356). Do jeito que está desenhado, nunca vai passar. Consertar o CI não basta: o portão de assinatura do release.yml precisa ser redesenhado.
- **CONFIRMADA**: (Lista do que existe) Voz de versões diferentes descartada sem ninguém dizer nada; o contador malformed registra.
  - LIDO. media.rs:184-190 exige `version != PROTOCOL_VERSION` → erro. O layout de 11 bytes é o mesmo desde o primeiro commit do arquivo (8df5da6, 09/08), com HEADER_LEN=11 e ssrc/seq/timestamp nos offsets 1/5/7.

LIDO. voice_room.rs:1057-1059 soma drops.malformed; voice.rs:1914 faz `continue`. **Correção:** Pior que o descrito. `VoiceRoom::drops()` (voice_room.rs:421) só é lido em testes: o grep em crates/seele-server/src não acha nenhum leitor em produção. É o «contador que ninguém lê» do CLAUDE.md, literalmente.
- **CONFIRMADA**: (Lista do que existe) Guarda contra abrir um banco migrado por versão mais nova — ausente.
  - LIDO. persistence/mod.rs:282-301 só pula as migrações ≤ current. Quando current passa do maior número conhecido, não há verificação: o banco abre calado. **Correção:** Hoje não causa dano. v0.14.2 e HEAD têm as mesmas 14 migrações (`grep -c 'version:'` em schema.rs nas duas) e o diff de persistence não acrescenta nenhuma. Só passa a importar quando houver versões lado a lado com migração nova.

### Severidades contestadas

- **Voz entre versões diferentes é descartada em silêncio**: o revisor propõe *importante (juntar ao bloqueante da janela de protocolo, não é um bloqueante próprio)*. Hoje não prejudica ninguém. Duas versões diferentes nem chegam à voz, porque o controle morre no Challenge ou no Hello. O defeito só aparece depois que a janela de controle funcionar, então ele é um sub-item da mesma entrega: sem ele, a janela de controle fica incompleta. Contado à parte, infla a lista de bloqueantes.
- **Recusa por versão aparece como falha de rede ou «PROTOCOLO VIOLADO»**: o revisor propõe *importante (barato; entregar junto com a subida de protocolo da 1.0)*. Uma frase errada não é dano grave em si, e a maior parte do conserto é do lado do servidor, que pode sair a qualquer momento. Um servidor novo que responda Disconnecting{Incompatible} carimbado com o byte do par (o erro UnsupportedVersion traz o `found`) conserta até clientes 0.11–0.15, que já estão em campo. O cliente 1.0 também consegue reconhecer servidor velho sem mudar o fio: um ProtocolViolation logo depois do Hello, num quadro com carimbo menor que o dele. A próxima vez que isso atinge todo mundo é a transição 0.15→1.0, então deve ir junto, mas não trava a 1.0 sozinho.
- **O app para macOS não é notarizado**: o revisor propõe *importante; vira bloqueante se a medição de TCC confirmar perda de permissão a cada atualização*. O primeiro contato está documentado no README (xattr ou Ajustes → Abrir Mesmo Assim). É atrito de uma vez só, contornável com instrução, e não dano grave. O que poderia torná-lo grave é o que o analista deixou como INFERIDO. Medi que o requisito designado da assinatura ad-hoc é `cdhash H"…"` (`codesign -d -r-`). Então cada atualização muda a identidade vista pelo TCC, e microfone e gravação de tela podem se perder ou pedir de novo a cada release, num ritmo de 9 releases em 18 dias. Isso precisa ser medido (instalar a 0.14.2, conceder, atualizar, testar voz e tela) antes de gastar US$ 99/ano e mexer no hardened runtime, que traz o risco do dlopen do OpenH264.

### O que o analista não viu

- A janela proposta é assimétrica. negotiate() recusa com PeerTooNew qualquer par mais novo (version.rs:221-226), o cliente sempre anuncia PROTOCOL_VERSION (client.rs:1667) e o seele-core não tem código que aprenda ou se adapte à versão do servidor (grep vazio). Carimbar pela versão negociada só resolve cliente velho → servidor novo. O caso «servidor persistente que ficou para trás» exige que o cliente desça de versão (faixa mín–máx no Hello ou nova tentativa em N−1). LIDO.
- O servidor 0.15 aceita hoje o Hello da 0.14: o carimbo 7 está na janela e Hello.version=7 passa (session.rs:467-491). Só depois ele manda um Challenge carimbado 8, que o 0.14 não lê. A janela do lado de quem ouve já dava ao servidor tudo o que ele precisava para recusar direito naquele ponto, e ele não recusa. Quem hospeda também não vê nada na tela. LIDO; o silêncio do lado de quem hospeda é INFERIDO.
- O aviso de quebra nunca chega ao app. O campo notes do latest.json é só «SEELE 0.15.0. As notas estão em https://…» (MEDIDO; empacotar/manifesto.py:113), e é isso que a tela de atualização mostra (tela-server.js:1285-1287). Quem aperta atualizar sozinho não vê «Atualizem juntos: a v0.15.0 e a v0.14 não conversam» e se tranca fora do grupo. É «o produto sabe e não conta».
- O seeled não tem canal de atualização: nada em crates/seele-server/src busca manifesto. Um servidor sempre ligado, que é o que o dono quer com a feature de persistência, fica para trás para sempre e nada avisa quem opera. Somado às quebras de protocolo, cada subida desliga em silêncio o servidor persistente para quem atualizou. LIDO.
- Link persistente e o v= do link se contradizem. O convite embute a versão de quem hospeda (hospedagem.rs:210-220; uri.rs:390-395), e o ADR 0046 decide abrir outra versão a partir disso (main.rs:6926-6932). Uma URL que não muda carrega uma versão velha depois da primeira atualização, ou precisa largar o v=. O ADR 0047 não trata versão (grep 'vers' só acha contexto irrelevante). LIDO; a consequência é INFERIDA.
- O README manda à «aba **Releases**» (README.md:119) do repositório de código, cujo Latest é a v0.10.0, e nunca cita SEELE-RELEASES (grep vazio). Ele também promete «.deb no Linux» (README.md:120, 171), e o release v0.15.0 não publica nada de Linux (MEDIDO na lista de assets). O app 0.10.0 se autoatualiza; o seeled e quem procura o Linux ficam sem saída.
- install.sh:71 consulta a API do GitHub sem autenticação, limitada a 60 requisições por hora por IP, e o limite estourou nesta sessão (MEDIDO). Nesse caso o script diz «não achei nenhuma versão publicada… Se o repositório for privado», um diagnóstico errado. Além disso, o comentário de install.sh:55 («Só o macOS publica pacote universal») é falso: macos.sh:5 e 41 geram só a arquitetura do host, então um Mac Intel recebe um seeled arm64.
- O CI vermelho é defeito de verdade. Na última execução falharam clippy (macos e windows) e test (linux, macos e windows), com exit 101 e 1 (MEDIDO na página da execução 35762838943). Enquanto isso, publicar.sh publica depois de uma bateria local que aceita --sem-bateria e --pular (publicar.sh:218, 691-695). O guarda automático existe e não roda; o que roda é o local, sem registro público. O release das notas diz que foi montado «nas máquinas de quem publica».

### Desenho proposto pelo analista

## A política de compatibilidade que a 1.0 promete

**1. Unidade.** O produto segue SemVer. O protocolo tem um inteiro `P` próprio. A 1.0 fixa `P₀ = 9`, a primeira versão que segue as regras abaixo.

**2. A promessa.** Dois builds 1.x cujos protocolos distem até **2** conversam por inteiro (texto, voz e tela), **nos dois sentidos**. Cada build fala `[P−2, P]`.
- `P` sobe no máximo uma vez por versão menor (1.1, 1.2…) e **nunca** numa correção (1.x.y).
- O limite inferior da janela só sobe quando a versão que sai dela já foi substituída há **60 dias** ou mais.
- Na prática: quem está até duas versões menores atrás continua conversando, e ninguém precisa combinar data de atualização.

**3. A regra do fio**, que torna a janela 2 barata:
- toda mudança é **só por acréscimo**: variante nova no fim da lista e motivo novo no fim do enum;
- nunca um campo novo numa variante ou struct existente: cria-se `XxxV2`;
- nunca remover nem reordenar;
- qualquer coisa fora disso é a 2.0.

Isso já vem sendo cumprido na prática desde a 0.11 (medido). A regra só passa a ser escrita e guardada.

**4. Quem atualiza primeiro.** Dentro da janela, ninguém coordena. Fora dela, **quem hospeda atualiza primeiro**, e o app ajuda sem violar o ADR 0026:
- procura atualização **ao apertar HOSPEDAR**, que é um ato, não um arranque;
- procura também **quando uma conexão é recusada por versão**, e mostra o botão ali mesmo.

**5. Fora da janela, a recusa é sempre legível** e diz as duas versões e quem precisa atualizar. Exemplo: «este servidor está na 1.3.0 e você na 1.0.2 — atualize» ou «peça a quem hospeda para atualizar». Nunca «nada respondeu».

**6. Dados.** Atualizar nunca apaga nada. Um build que encontra um banco mais novo que ele **recusa hospedar e diz por quê**.

**7. Plataformas prometidas.** macOS 11+ Apple Silicon (e Intel, se o universal entrar) e Windows 10/11 x64. Linux: compilar do código-fonte, ou `.deb` «voz e texto, sem tela». Tudo isso escrito na página, sem deixar descobrir.

## O que falta em código, em ordem (cada etapa reaproveita o que existe)

### Etapa 0: horas, antes de qualquer outra coisa
- `install.sh:31` e `install.ps1:28` → `DATA-AND-DEV/SEELE-RELEASES`.
- Remover o endpoint secundário do atualizador em `tauri.conf.json`, ou publicar ali o mesmo `latest.json`.
- README e `.github/NOTAS-DE-RELEASE.md`:
  - tirar `connection`, `.deb` e «três programas»;
  - trocar «botão direito → Abrir» por «Ajustes → Privacidade e Segurança → Abrir Mesmo Assim»;
  - declarar Mac Intel e Linux como fora do pacote.
- Esconder o botão BAIXAR de CONFIGURAÇÕES · VERSÕES e a frase de `index.html:534-537` até o manifesto v2 existir.

### Etapa 1 (M): a recusa certa. É o que muda a experiência já na primeira 1.x
- **Servidor** (`session.rs:467-491`):
  - ler o Hello com o layout congelado (medido estável desde a v0.10.5-1), sem negociar pelo carimbo;
  - para um par fora da janela, responder `Disconnecting{Incompatible}` **carimbado com o byte que o par anunciou**. Os ordinais 11 e 0 são estáveis desde a 0.11, então até um 0.11–0.15 lê a frase certa;
  - fechar a conexão QUIC com um código de aplicação reservado e o motivo `proto=9 min=7 produto=1.0.0`, legível por qualquer build futuro sem passar pelo postcard.
- **Cliente** (`client.rs:1678-1681`):
  - `ControlError::UnsupportedVersion{found}` vira `ConnectError::Incompatible{…}`, e não `Unreachable`;
  - ler o código de fechamento para montar a frase com as duas versões (`frases.js`).
- **Guarda do banco:** em `persistence/mod.rs:282`, se `current > max(MIGRATIONS)`, recusar hospedar com uma frase.

### Etapa 2 (G): a janela de verdade. Resolve a pendência #42 e precisa de um ADR que substitua o 0046 na parte de compatibilidade
- **`seele-proto`:**
  - `encode_em(versao, msg)`, e `decode` aceitando carimbos em `[min, max]`;
  - `negotiate` passa a devolver `min(par, nosso)` quando esse valor estiver na janela das duas pontas.
- **Cliente:**
  - carimba o Hello com o **mínimo** que fala, e manda `Hello.version` = máximo;
  - diante de `Incompatible` de um servidor legado (≤ 0.15), tenta de novo uma vez com `Hello.version` rebaixado. O convite não é gasto, porque a versão é conferida antes da portaria (`session.rs:488` vem antes de `:494`);
  - carimba os quadros com a versão negociada e ganha uma tabela de verbos por versão (hoje vazia: o vocabulário do cliente não muda desde a 0.11).
- **Servidor:** grava a versão negociada em `session.protocol_version` (já existe) e a usa em todo `frame::write`, passando a versão em `crates/seele-server/src/frame.rs` e `crates/seele-core/src/frame.rs`. O calar por versão já existe (`session.rs:4177`).
- **Voz:** `MEDIA_HEADER_VERSION` próprio. O servidor aceita qualquer cabeçalho dentro da janela e reescreve o byte 0 por assinante (`voice_room.rs:1116` já copia por assinante).
- **Janela inicial da 1.0:** `[7, 9]`. Se a medida confirmar (os tipos aninhados não foram verificados), um host 1.0 serve clientes 0.14 e 0.15, e **a 1.0 não vira a quarta quebra**.
- **O guarda que prova, e não só existe:** um `vetores-de-protocolo.json`, no mesmo espírito do `vetores-de-hash.json`, com os bytes de cada variante em cada versão da janela, decodificados pelo código atual em todo teste. Revertendo o carimbo negociado, o teste tem de reprovar.

### Etapa 3 (P): o produto conta antes de conectar
- O link ganha `p=<protocolo>` (`uri.rs:394`). `analisar_convite` compara com a janela local e avisa antes de discar.
- `manifesto.py` passa a emitir `unit.protocol`, `min_protocol`, `sha256` e `executable` (`seele-app` no macOS), acumulando as versões anteriores (lacunas 1 a 4 e 6 de `docs/versoes-lado-a-lado.md`).
- A tela de atualização passa a dizer «esta versão conversa com servidores a partir da X».

### Etapa 4 (P): a publicação enxerga o protocolo
- `publicar.sh` lê `PROTOCOL_VERSION` e `MIGRATIONS` da tag anterior.
- Se o protocolo mudou, exige nas notas um bloco de compatibilidade. Recusa publicar se o novo `min` excluir a release publicada há menos de 60 dias.
- O classificador deixa de afirmar «nenhuma mudança de produto» quando o protocolo ou as migrações mudaram.

### Etapa 5 (M, US$ 99/ano): notarização no macOS
- Developer ID, `hardenedRuntime: true`, o entitlement `disable-library-validation` para o OpenH264, e o `seeled` do sidecar assinado.
- `notarytool submit --wait` e `stapler staple` em `macos.sh`.
- Medir que o microfone e a gravação de tela continuam permitidos depois de uma atualização.

### Etapa 6 (M, $): Windows
- Certificado OV com assinatura em nuvem (o Azure não atende o Brasil), no `signCommand` do Tauri ou em `windows.ps1`, cobrindo o `seele-instalador`, o `seele-app` e o `seeled`.

### Etapa 7 (P): licença
- Decidir a licença, gerar `LICENSE` e THIRD-PARTY (`cargo deny list` ou cargo-about) e embuti-los no bundle.
- OpenH264: aviso, interruptor e reprodução da licença da Cisco.
- Campo `license` no `mod.json` da API 6.

### Etapa 8 (M): CI
- Descobrir por que está vermelho, torná-lo verde e religar em push e PR na `main`.
- Pôr o `cargo deny check` no CI.
- Completar o segredo do atualizador no GitHub.

### Depois da 1.0
- Mac universal, `.deb` publicado a partir do artefato do CI, e PATH no Windows.
- ADR 0046 (launcher) só para «hospedar numa versão antiga por causa de MOD», e só com o manifesto v2 e um pacote que se abra numa pasta no Windows. Deixa de ser o mecanismo de compatibilidade, porque um processo por versão contradiz a multiconexão do ADR 0031.

**Dependências:** 0 → 1 → 2 → 3/4. As etapas 5 a 8 correm em paralelo, e as 5 e 6 dependem de compra e de validação de identidade (dias). O mínimo para a 1.0 nesta frente são as etapas 0, 1, 2 e 5, mais a decisão da 7.

### Perguntas ao dono

- Quais plataformas a 1.0 promete oficialmente: só macOS Apple Silicon e Windows x64? O Mac Intel entra (build universal, dobra o tempo de empacotamento)? O Linux entra como «voz e texto, sem tela»?
- Você topa pagar o Apple Developer Program (US$ 99/ano) antes da 1.0? Sem notarização, o primeiro contato de um amigo no macOS é «Mover para o Lixo», e o atalho do botão direito não resolve mais.
- A assinatura no Windows seria em nome de pessoa física ou de empresa (há CNPJ)? E em que país? O Azure Artifact Signing, que é o plano do ADR 0026, só atende organizações de 12 países/regiões e pessoas físicas nos EUA e no Canadá. Aceita um certificado OV de CA, com custo anual maior?
- Qual licença você quer: aberta (MIT/Apache-2.0, MPL-2.0, GPL-3.0/AGPL-3.0) ou código disponível, mas proprietário? E qual licença o indexador vai exigir de MODs de terceiros?
- Aceita tirar o launcher do ADR 0046 (versões lado a lado) do caminho da 1.0 e trocá-lo por uma janela de protocolo de 2 versões? O launcher roda um processo por versão, o que não combina com a conexão simultânea numa janela só.
- Qual o ritmo de releases depois da 1.0? Hoje foram 10 releases em 17 dias e 3 quebras de protocolo. A janela de 2 versões menores e a carência de 60 dias pressupõem no máximo uma subida de protocolo por versão menor.
- Onde estão a chave privada do atualizador e a senha dela, e quantas cópias existem? Há uma segunda pessoa com acesso? Perdê-la tranca a atualização de todos os instalados.
- O CI deve voltar a rodar sozinho em push e PR, e ser obrigatório para publicar? Ou o processo manual (publicar.sh no seu Mac mais a máquina Windows) passa a ser o oficial da 1.0?
- Algum servidor seeled persistente (VPS, máquina sempre ligada) já roda para o seu grupo? Se sim, ele foi instalado pelo install.sh? Hoje esse script instala a 0.10.0.

---

## Experiência de ponta a ponta

**Resumo do analista.** O miolo da jornada existe e está publicado na v0.15.0 (= HEAD e2fac4d): hospedar com servidores guardados, link com impressão digital e versão, portaria com espera que tenta sozinha, texto com estado de envio, anexos, tela, faixa de queda de 5 min e F01/F02. O que faria um amigo não técnico desistir na primeira noite está nas bordas. Nada vem assinado, e o README ensina um contorno do macOS que o próprio macOS já tirou. O `seele://` não é clicável no WhatsApp. O microfone nasce em TECLA sem a tela dizer qual tecla, e ela só funciona com a janela em foco e fora da caixa de texto. Não há cancelamento de eco nem aviso de fone em tela nenhuma. E quem hospeda derruba todo mundo ao fechar a janela, sem pergunta, enquanto os convidados leem «CONEXÃO PERDIDA · RECONECTANDO» por 5 minutos sem saber que o servidor foi desligado. Também faltam notificações do sistema (portaria, mensagem, entrada na sala), e a contagem de não lidas existe no core sem chegar à tela. Recomendo, antes de qualquer feature nova, uma rodada curta de casca e textos (P), mais a despedida do servidor e a confirmação ao fechar a janela (M), mais as notificações (M). Assinatura e uma sessão de voz com gente de verdade entram como pré-condições da 1.0. Deep link com uma página https de convite resolve de uma vez «link clicável» e «URL amigável».

### O que existe

- `construido_e_publicado`: HOSPEDAR AQUI com lista de servidores guardados, PREPARAR (nome, imagem, versão que vai hospedar) e entrada sem conferir a própria chave (apps/seele-app/ui/tela-boot.js:618-700, 819-1010; apps/seele-app/src/main.rs:1505-1640; apps/seele-app/src/servidores.rs)
- `construido_e_publicado`: Alcance dito junto ao link (degraus do ADR 0022) e botão ABRIR A PORTA AGORA no Windows (apps/seele-app/ui/tela-boot.js:306-409; apps/seele-app/ui/frases.js:738-788; apps/seele-app/src/main.rs:1760-1830)
- `construido_e_publicado`: Link seele:// com impressão digital, caminhos alternativos, bilhete de encontro e versão (&v=) (crates/seele-server/src/hospedagem.rs:218-219, 270-292)
- `construido_e_publicado`: Colar link ou endereço no diálogo CONECTAR / ONDE VOCÊ JÁ ESTEVE (apps/seele-app/ui/camada-servidores.js:161-213; apps/seele-app/ui/index.html:3722-3740)
- `ausente`: Esquema seele:// registrado no sistema (link clicável) (apps/seele-app/Info.plist (sem CFBundleURLTypes); apps/seele-app/Cargo.toml (sem deep-link/single-instance); docs/pendencias.md:427-431 (#10))
- `parcial`: Abrir o app já entrando num link (--entrar <link>) (apps/seele-app/src/main.rs:278-293; apps/seele-app/ui/tela-boot.js:567-591 (hoje só usado pelo lançador de versões))
- `construido_e_publicado`: Portaria ligada ao hospedar, espera do convidado que tenta sozinha, faixa «está batendo» dentro da janela (apps/seele-app/src/main.rs:1582-1607; apps/seele-app/ui/tela-auth.js:534+; apps/seele-app/ui/camada-portaria.js:436-520)
- `ausente`: Notificação do sistema, som de entrada/mensagem, contagem no ícone, bandeja (apps/seele-app/ui/camada-portaria.js:456-458; apps/seele-app/Cargo.toml; apps/seele-app/ui/index.html:2105-2107)
- `construido_mas_nao_ligado`: Não lidas por canal (crates/seele-core/src/state.rs:741 (sem consumidor em seele-ffi, main.rs ou ui/))
- `construido_e_publicado`: Quarto MORO/QUEM: o link antigo volta a achar o servidor pela impressão digital (apps/seele-app/src/main.rs:998-1027; crates/seele-ffi/src/lib.rs:7797 (onde_mora_hoje); VPS respondendo segundo o coordenador)
- `construido_e_publicado`: F01/F02: redução de ruído ligada, sensibilidade da voz medida pela sala, testar o microfone, OUVIR-ME (crates/seele-audio/src/supressao.rs; apps/seele-app/ui/index.html:2630-2690; empacotar/notas/0.15.0.md:44-68 (validação com voz humana pendente))
- `ausente`: Cancelamento de eco (AEC) (crates/seele-audio/src/ganho.rs:14-17 cita seam `--features aec` que não existe em crates/seele-audio/Cargo.toml; README.md:197-198)
- `construido_e_publicado`: Aperte para falar com tecla configurável; soltar sempre fecha (R03) (apps/seele-app/ui/tela-sessao.js:3397-3446, 4290-4327)
- `ausente`: Aperte para falar global (fora da janela) (apps/seele-app/Cargo.toml (sem global-shortcut); tela-sessao.js:4324 fecha no blur)
- `construido_e_publicado`: Mudo, isolamento total e volume por pessoa (apps/seele-app/ui/tela-sessao.js:838-896; apps/seele-app/ui/tela-chamada.js:420-539, 819-831)
- `construido_e_publicado`: Texto com estado de envio/TENTAR DE NOVO, rascunho por canal, botão ENVIAR, anexos (empacotar/notas/0.15.0.md:76-83; apps/seele-app/ui/index.html:1601; docs/jornadas-pendentes-2026-09-20.md:19)
- `construido_e_publicado`: Faixa de queda com contagem de 5 min e tela de fim com RECONECTAR (apps/seele-app/ui/index.html:730-750, 3605-3613; apps/seele-app/ui/tela-fim.js)
- `ausente`: Aviso aos convidados de que o servidor foi encerrado (ServerShuttingDown difundido) (crates/seele-server/src/lib.rs:886-891 («the close is abrupt»); session.rs:784)
- `ausente`: Confirmação ao fechar a janela quando hospedando / continuar hospedando em segundo plano (apps/seele-app/src/main.rs:8119-8153 (só ExitRequested → desconecta o cliente))
- `parcial`: Checagem da permissão do microfone com botão para os Ajustes (apps/seele-app/ui/tela-server.js:1623-1643 (só em CONFIGURAÇÕES); apps/seele-app/src/main.rs:6513-6550)
- `parcial`: Atualizador dentro do app (apps/seele-app/ui/tela-server.js:1353-1435 (só por clique, sem checagem ao abrir))
- `so_desenho`: Assinatura de código do SO (Apple Developer ID/notarização, Azure Artifact Signing) (.github/workflows/release.yml:293-365, 515-530 (workflow_dispatch; releases saem de empacotar/publicar.sh, que não assina))
- `so_desenho`: Pacotes Mac Intel/universal e Linux .deb (.github/workflows/release.yml (universal); página de assets da v0.15.0 só traz aarch64.dmg e x64.exe (MEDIDO))
- `ausente`: Pedir o nome no primeiro uso (apps/seele-app/src/main.rs:1050-1057 (recurso pessoa-XXXX); apps/seele-app/ui/tela-boot.js:762-799)
- `ausente`: Chamada privada / mensagem direta; conexão a vários servidores (sem código (grep); apps/seele-app/src/main.rs:898-899 AlreadyConnected; tela-sessao.js:3493; docs/adr/0047 §3-4)

### Lacunas

- **[BLOQUEANTE · esforço M · risco baixo] Instalar exige vencer o sistema operacional: nada é assinado, e o contorno do macOS ensinado no README já não existe.** O amigo baixa o .dmg ou o .exe e o sistema recusa.
- **macOS 15+ (INFERIDO):** o botão direito → Abrir que o README ensina não funciona mais; o caminho é Ajustes › Privacidade e Segurança › Abrir Mesmo Assim, que ninguém descobre sozinho.
- **Windows 11 com Smart App Control:** o programa simplesmente não abre, sem contorno. O próprio repositório registra isso no primeiro teste com máquina de outra pessoa.
- **Toda atualização no Mac:** a assinatura ad hoc faz microfone, rede local e gravação de tela serem pedidos de novo.

O caminho que assina está escrito no release.yml, mas as releases saem do publicar.sh, que não assina.
  - *Evidência:* LIDO:
- apps/seele-app/tauri.conf.json: `signingIdentity: "-"`, `hardenedRuntime: false`;
- empacotar/windows.ps1:18-19, 214 só assinam com minisign; publicar.sh sem Authenticode (grep);
- .github/workflows/release.yml:293-365 (caminho com assinatura, só em workflow_dispatch);
- docs/assinatura-e-atualizacao.md:78-92 (Smart App Control);
- README.md:124-130;
- comentário do apps/seele-app/Info.plist («a pergunta volta a cada atualização»).

MEDIDO: a página da v0.15.0 diz «Fora da integração contínua… Windows numa máquina Windows alcançada por SSH».
- **[BLOQUEANTE · esforço M · risco medio] O link seele:// não é clicável e não ensina nada a quem ainda não tem o SEELE.** No WhatsApp o convite chega como um texto longo e não clicável: `seele://IP:8383?fp=<64 hex>&alt=…&enc=…&v=…`. A única entrada é:
1. copiar o texto (muitas vezes no celular);
2. abrir o SEELE no PC;
3. clicar em CONECTAR;
4. colar e apertar ENTRAR.

Quem ainda não instalou recebe algo que parece spam. Nenhum esquema está registrado no sistema.

A mesma peça resolve a «URL amigável» pedida pelo dono: uma página https que o WhatsApp torna clicável e que oferece abrir no app, baixar ou copiar.
  - *Evidência:* LIDO:
- apps/seele-app/Info.plist sem CFBundleURLTypes;
- apps/seele-app/Cargo.toml sem tauri-plugin-deep-link e sem single-instance;
- docs/pendencias.md:427-431 (#10 aberta);
- a base existe: `--entrar` em apps/seele-app/src/main.rs:278-293 e tela-boot.js:567-591;
- instrução do diálogo: apps/seele-app/ui/index.html:3736-3737.

INFERIDO: o WhatsApp só torna clicáveis http(s), www, tel e mailto.
- **[BLOQUEANTE · esforço P · risco baixo] Eco garantido para quem usa o alto-falante, e nenhuma tela manda usar fone.** Não há cancelamento de eco. Um amigo de notebook sem fone, no modo VOZ, devolve a conversa para todo mundo: ninguém sabe de onde vem o eco, e o portão abre com a voz dos outros.

O README exige fones dos dois lados; o app não diz isso em lugar nenhum. A redução de ruído F02 não resolve eco.

O mínimo para a 1.0 é um aviso em tela, de esforço P. O AEC de verdade é GG e passa por rever o ADR 0007.
  - *Evidência:* LIDO:
- README.md:197-198;
- crates/seele-audio/src/supressao.rs:4-6 («não cancelamento de eco»);
- crates/seele-audio/src/ganho.rs:14-17 cita um `--features aec` que não existe em crates/seele-audio/Cargo.toml (sem [features]);
- grep por fone/eco/realimenta nas telas só acha o OUVIR-ME (apps/seele-app/ui/index.html:2727).
- **[BLOQUEANTE · esforço P · risco medio] «Ninguém me ouve»: o padrão é TECLA, a tela não diz qual tecla, e ela só vale com a janela em foco e fora da caixa de texto.** O amigo entra na sala, fala, e ninguém ouve. A linha de estado diz «MICROFONE ABRE NA TECLA · OUVINDO» sem nomear ESPAÇO.

Mesmo depois de descobrir a tecla:
- Espaço com o foco na caixa de texto digita espaço — o caso comum depois de mandar uma mensagem;
- a tecla não funciona com o jogo ou o navegador em foco, porque perder o foco fecha o microfone;
- não existe tecla global.

É a principal razão provável de uma primeira noite frustrada.
  - *Evidência:* LIDO:
- crates/seele-core/src/voice.rs:867 (padrão PushToTalk);
- apps/seele-app/src/main.rs:1124-1131;
- apps/seele-app/ui/tela-sessao.js:841 e 887-896 (texto sem a tecla);
- apps/seele-app/ui/tela-sessao.js:4290-4295 (`keydown` em window com `!digitando()`) e 4324-4327 (blur fecha);
- apps/seele-app/ui/base.js:107-110;
- tecla só aparece na ajuda (apps/seele-app/ui/index.html:3878);
- Cargo.toml sem global-shortcut.
- **[BLOQUEANTE · esforço M · risco medio] Quem hospeda fecha a janela e derruba todo mundo, sem pergunta e sem avisar os convidados.** No Mac, fechar a janela costuma não encerrar o app, e o anfitrião a fecha achando que vai para segundo plano. Aqui ela encerra o processo e o servidor junto.

O que acontece:
- não há `CloseRequested`, confirmação nem bandeja;
- a despedida só desconecta o cliente, e o servidor fecha de forma abrupta, sem motivo no fio;
- os convidados veem 5 minutos de «CONEXÃO PERDIDA · RECONECTANDO / Sessão mantida em memória local» e depois «ENLACE ENCERRADO». Nada diz «quem hospeda desligou».

O botão SAIR DO SERVIDOR, esse sim, pergunta e escreve a consequência.
  - *Evidência:* LIDO:
- apps/seele-app/src/main.rs:8119-8124 e 8144-8153 (despedir_se só chama connection.disconnect());
- crates/seele-server/src/lib.rs:886-891 («the close is abrupt»);
- crates/seele-server/src/session.rs:784 (único uso de ServerShuttingDown);
- crates/seele-core/src/enlace.rs:289-294 e 5150-5156;
- apps/seele-app/ui/index.html:730-750 e 3609;
- contraste: apps/seele-app/ui/tela-chamada.js:765-784.
- **[BLOQUEANTE · esforço M · risco baixo] A voz padrão da 0.15 (supressão ligada, portão novo) nunca foi ouvida por uma pessoa.** A 0.15 mexeu no caminho de captura inteiro — supressão antes do portão, limiar medido pela sala, retenção de 40 ms, carimbos de tempo — e ligou a supressão por padrão. A calibração foi feita só com sinal sintético.

O README ainda diz que voz real entre duas máquinas não foi verificada, e não há registro em m1-medicoes. Com a 1.0, a primeira impressão de todo amigo é esse caminho.

O roteiro de teste já existe; falta executá-lo com gente, notebook sem fone e headset, e decidir com base nele o padrão TECLA/VOZ e a força da supressão.
  - *Evidência:* LIDO:
- empacotar/notas/0.15.0.md:65-68 («ainda não com gravações de voz de gente»);
- docs/entrega-review-v15-2026-09-22.md (F01/F02 «parcial … Falta a validação acústica»);
- docs/auditoria-f01-f02-2026-09-22.md;
- README.md:288-290;
- docs/teste-duas-maquinas.md (roteiro).

INFERIDO: há uso real, pelos relatos citados em crates/seele-audio/src/ganho.rs:7-9.
- **[importante · esforço M · risco baixo] Nada alcança quem está com a janela minimizada: batida na portaria, mensagem, alguém entrando na sala.** O amigo bate e quem hospeda, jogando em tela cheia, não vê. A tela do amigo manda «avisar quem hospeda por outro canal» — ou seja, pelo WhatsApp.

Faltam:
- notificação do sistema;
- contagem no ícone ou no título;
- som de entrada e saída.

A contagem de não lidas por canal já existe no core e não chega à tela.
  - *Evidência:* LIDO:
- apps/seele-app/ui/camada-portaria.js:456-458 (tauri-plugin-notification «não está nas dependências»);
- docs/pendencias.md:2851 (#23);
- apps/seele-app/ui/index.html:2105-2107 (AVISO SONORO sem nada atrás);
- crates/seele-core/src/state.rs:741 `nao_lidas` sem consumidor (grep vazio em seele-ffi, main.rs e ui/);
- texto da espera: apps/seele-app/ui/index.html:1987-1988.
- **[importante · esforço P · risco baixo] Versão incompatível: o app sabe a versão do servidor e não conta, e atualizar é só por clique.** O protocolo sobe a cada versão menor (a 0.14 e a 0.15 não conversam). O link traz `&v=<versão>`, mas quando ela não está instalada a tela a ignora e conecta, e a falha vira «Um dos dois lados está desatualizado». O app podia dizer «o servidor está na 0.15.0 e você na 0.14.2», com um botão ATUALIZAR.

Também:
- ninguém procura atualização ao abrir o app;
- a nota da 0.15 promete guardar a 0.14.2 «ao lado», o que no Windows não existe.
  - *Evidência:* LIDO:
- crates/seele-server/src/hospedagem.rs:218-219;
- apps/seele-app/src/main.rs:6925-6932;
- apps/seele-app/ui/camada-servidores.js:187;
- apps/seele-app/ui/frases.js:32-34;
- apps/seele-app/ui/tela-server.js:1435 (único gatilho de procurar atualização);
- apps/seele-app/src/versoes.rs:291-296.

MEDIDO: a página da v0.15.0 repete empacotar/notas/0.15.0.md:11-14 sem ressalva para o Windows.
- **[importante · esforço P · risco baixo] Primeiro uso sem nome: o amigo entra e bate na portaria como `pessoa-3f2a`.** Ninguém pede o nome no primeiro uso. O rodapé da entrada mostra «—», e o connect deriva `pessoa-XXXX`.

Quem hospeda vê um desconhecido batendo à porta e tem de adivinhar quem é. Na sala, todos aparecem com esse nome até alguém achar PERFIL.
  - *Evidência:* LIDO:
- apps/seele-app/src/main.rs:1050-1057;
- apps/seele-app/ui/index.html:424;
- apps/seele-app/ui/tela-boot.js:762-799 (abrirPerfil só no clique);
- apps/seele-app/ui/camada-portaria.js:91-118.
- **[importante · esforço P · risco baixo] Microfone negado ou mudo pelo sistema só é diagnosticado dentro de CONFIGURAÇÕES.** Em duas situações o microfone do amigo não chega à conversa:
- no Windows, com o interruptor de privacidade desligado;
- no Mac, com a permissão negada ou esquecida numa atualização ad hoc.

Nos dois casos ele fala sozinho, e a sessão diz «no ar» enquanto a tecla está segurada. A checagem de permissão e o botão para os Ajustes existem, mas só rodam ao abrir CONFIGURAÇÕES. Nada detecta captura em silêncio.
  - *Evidência:* LIDO:
- apps/seele-app/ui/tela-server.js:726, 1570 e 1623-1643 (únicos chamadores);
- relato «falando sozinha» em crates/seele-audio/src/device.rs:1276-1280;
- apps/seele-app/ui/tela-sessao.js:887-896;
- grep sem detecção de silêncio digital no caminho de voz.
- **[importante · esforço M · risco medio] Hospedar atrás de NAT difícil termina em «SÓ FUNCIONA NA SUA REDE» sem próximo passo; o convidado não distingue desligado de inalcançável.** **Quem hospeda.** Com CGNAT mais NAT simétrico, recebe «ESTE LINK SÓ FUNCIONA NA SUA REDE.» e nenhuma ação; a retransmissão está fora por decisão.

**O convidado.** Recebe «OS PACOTES SAÍRAM E NADA VOLTOU / Confira… a porta UDP» — um beco para quem não é técnico.

**O que o produto sabe e não conta.** Quando o ponto de encontro responde mas não conhece o servidor, isso é forte indício de «quem hospeda está com o SEELE fechado», e a resposta é descartada.

Não há medida de quantos anfitriões caem nesse degrau.
  - *Evidência:* LIDO:
- apps/seele-app/ui/frases.js:785-788 e a frase SemResposta no mesmo arquivo;
- apps/seele-app/src/main.rs:1008-1027 (resposta ausente do quarto é ignorada);
- ADR 0022 (degrau 5 fora de escopo).

INFERIDO: o QUEM de um servidor desconhecido não responde (crates/seele-proto/src/encontro.rs).
- **[importante · esforço P · risco baixo] A frase do furo de NAT manda gerar outro link, e o quarto agora faz o link velho voltar.** Quem hospeda lê «O link vale enquanto o app estiver aberto; se fechar, gere outro.» e passa a mandar um link novo toda noite.

O cliente, porém, já pergunta ao quarto pela impressão digital do link colado ou guardado, e o coordenador mediu o quarto respondendo. A frase precisa de um teste de campo — fechar, reabrir o mesmo servidor, colar o link de ontem — e então ser trocada. Senão o produto desanima um uso que ele já sustenta.
  - *Evidência:* LIDO:
- apps/seele-app/ui/frases.js:767-770;
- apps/seele-app/src/main.rs:998-1027;
- docs/adr/0047-o-link-que-volta-a-funcionar-amanha.md §2.1.

MEDIDO pelo coordenador: `sondar` com MORO/QUEM vivos em encontro.seele.app.br:8384.
- **[importante · esforço M · risco alto] A abertura do áudio não tem prazo e prende a entrada em «conectando» para sempre.** `Voice::start_preferring` roda dentro do connect, antes da resposta à tela. Se o CoreAudio bloquear — medido na cópia QA —, o botão fica desabilitado e a etapa muda sem explicação.

É o pior tipo de falha para quem não é técnico: nenhuma frase, só um app que parece travado.
  - *Evidência:* LIDO:
- crates/seele-ffi/src/lib.rs:3930-3951;
- docs/jornadas-pendentes-2026-09-20.md:81-86;
- sem prazo em crates/seele-audio/src/device.rs (grep por prazo e timeout).
- **[importante · esforço G · risco medio] Tela entre duas máquinas Windows quebrada desde a 0.8.5, e o som da tela no Windows leva a conversa junto.** A entrada promete «voz, vídeo e texto». Num grupo majoritariamente Windows, compartilhar a tela não aparece para quem assiste (regressão aberta).

No Windows, compartilhar a tela inteira com som faz quem assiste ouvir a própria voz de volta. A tela avisa, o que é certo, mas o recurso fica pela metade. «Vídeo» também sugere câmera, que não existe.
  - *Evidência:* LIDO:
- docs/pendencias.md:5786+ (#33 sem fechamento);
- empacotar/notas/0.15.0.md:16-25;
- docs/entrega-review-v15-2026-09-22.md (R21 parcial);
- apps/seele-app/ui/index.html:273 e 291.
- **[importante · esforço M · risco baixo] Mac Intel e Linux sem pacote.** O amigo com Mac Intel ou Linux não tem o que instalar, e o README ainda promete .deb.

Como a 0.14 e a 0.15 não conversam, ele também não consegue ficar numa versão antiga para participar.
  - *Evidência:* MEDIDO: a página de assets das v0.15.0, v0.14.2 e v0.14.0 só traz `aarch64.dmg` e `x64-instalador.exe`, e diz «Linux e Mac Intel ainda não têm pacote».

LIDO: README.md:119-122.
- **[importante · esforço M · risco medio] Identidade presa à máquina: trocar de PC custa o apelido, e um servidor que perde a chave tranca todos sem saída.** **Trocar de computador.** Reinstalar ou usar outra máquina gera uma chave nova, e o servidor responde «ESTE APELIDO JÁ É DE OUTRA PESSOA». Não há como liberar o nome nem exportar a identidade.

**Anfitrião que perde a pasta de configuração.** Todos os convidados recebem «A CHAVE DO SERVIDOR MUDOU… Confirme por outro canal antes de continuar». Não existe «continuar»: esquecer o servidor não desfaz o pino, e o TOFU recusa `Changed` mesmo com um link válido.
  - *Evidência:* LIDO:
- crates/seele-server/src/permissions.rs:213;
- crates/seele-core/src/conhecidos.rs:344-347;
- apps/seele-app/ui/frases.js:411-417;
- crates/seele-core/src/tofu.rs:141-148.
- **[importante · esforço P · risco baixo] O volume por pessoa pode mentir depois de uma reconexão.** A tela guarda o volume por apelido num mapa que nunca é limpo. O core aplica o ganho por `ssrc`, numa voz recriada a cada sessão.

Depois de uma queda, o controle continua mostrando 150% e a pessoa toca a 100%, sem nada na tela dizer. INFERIDO: falta conferir se o ssrc muda na reconexão.
  - *Evidência:* LIDO:
- apps/seele-app/ui/tela-sessao.js:143;
- apps/seele-app/ui/tela-chamada.js:524-539 e 819-831;
- crates/seele-core/src/voice.rs:1448-1451;
- crates/seele-ffi/src/lib.rs:2468-2485.
- **[desejável · esforço P · risco baixo] Textos de console justamente onde a pessoa decide.** Trocas sugeridas:
- «ENLACE ENCERRADO/ENLACE PERDIDO» → «A CONVERSA CAIU»;
- «RECONECTAR — 189.x.x.x:8383» → o nome do servidor;
- «A PORTA DESTE SERVIDOR», «impressão digital do certificado» e «as três camadas da porta» → linguagem de uso;
- tirar «A·02 / ENTRADA NO SERVIDOR» e «C·02 / SERVIDOR» da tela do convidado.

O diálogo do link também não diz o essencial: «quem usar este link vai bater à porta, e você aprova aqui».
  - *Evidência:* LIDO:
- apps/seele-app/ui/index.html:3609, 3671-3690, 1905-1940;
- apps/seele-app/ui/tela-fim.js (desenharSaidas).
- **[desejável · esforço GG · risco alto] Chamada privada e conexão a vários servidores não existem.** Não há mensagem direta nem chamada a dois. Uma sala com senha é o que mais se aproxima, e só quem administra a cria.

Quem hospeda não pode visitar outro servidor sem derrubar o próprio. A tela diz isso com honestidade («Dá para estar em um servidor por vez»). O desenho está no ADR 0047 §3-4.
  - *Evidência:* LIDO:
- sem código (grep por mensagem direta, chamada privada e whisper);
- apps/seele-app/src/main.rs:898-899;
- apps/seele-app/ui/tela-sessao.js:3488-3500.
- **[desejável · esforço P · risco baixo] Documentos e notas que contradizem o produto publicado.** Contradições encontradas:
- o README diz que a tela «ainda não [foi] construída»;
- o README ensina o contorno do macOS que já não existe;
- a página da v0.15.0 diz «Esta versão não traz nenhuma mudança de produto» logo abaixo das notas do protocolo 8 e de F01/F02;
- a nota da 0.15 promete versões lado a lado sem ressalva para o Windows;
- comentários dizem que a caixa do firewall «nasce desmarcada», e o instalador a marca por padrão;
- ganho.rs cita um seam de AEC que não existe;
- a pendência #23 diz que repetir a batida foi recusado, e a espera hoje repete sozinha.
  - *Evidência:* LIDO:
- README.md:127-128 e 232;
- apps/seele-app/src/main.rs:1776-1778;
- apps/seele-instalador/src/instalacao.rs:41 (`porta.unwrap_or(true)`);
- crates/seele-audio/src/ganho.rs:16-17;
- apps/seele-app/ui/index.html:2099-2100;
- docs/pendencias.md:2851+.

MEDIDO: o texto da página da release v0.15.0.
- **[desejável · esforço P · risco baixo] Voltar à conversa custa três cliques todo dia.** O app não oferece voltar ao último servidor ao abrir, nem iniciar com o sistema. O amigo precisa abrir o app, clicar em CONECTAR e clicar na linha do servidor.

Quem hospeda também precisa lembrar de abrir o SEELE para os outros entrarem.
  - *Evidência:* LIDO:
- apps/seele-app/ui/tela-boot.js:757-759 (CONECTAR abre o diálogo);
- sem autostart ou retomada em apps/seele-app/src/main.rs (grep por tray e autostart).

### O que o revisor conferiu

- **CONFIRMADA**: 1. Quem recebe o convite pelo WhatsApp só entra copiando e colando no diálogo CONECTAR: nenhum esquema está registrado e o link não é clicável.
  - LIDO: apps/seele-app/Info.plist (lido inteiro): só NSLocalNetwork/NSScreenCapture/NSMicrophoneUsageDescription, sem CFBundleURLTypes. apps/seele-app/tauri.conf.json: bloco `plugins` só tem `updater`, sem `deep-link`, então o laço `{{#each deep_link_protocols}}` do modelo NSIS (apps/seele-app/instalador.nsi:747-751) sai vazio. apps/seele-app/Cargo.toml: sem tauri-plugin-deep-link nem single-instance. apps/seele-instalador/src/janela.rs:271-275 diz que `seele://` «só existe como nome de canal interno de evento» e por isso a opção saiu do instalador. docs/pendencias.md:427-431 (#10 aberta). A única porta de entrada é apps/seele-app/ui/camada-servidores.js:160-199. O comportamento do WhatsApp não foi medido, mas não muda a conclusão: sem esquema registrado, o sistema não tem a quem entregar o clique, mesmo que o link virasse clicável. **Correção:** Um detalhe piora o caso: o campo só reconhece o convite quando o texto colado COMEÇA com `seele://` (camada-servidores.js:168 `escrito.startsWith("seele://")`). Uma mensagem copiada inteira, do tipo «entra aí: seele://…», cai em `irParaOServidor(alvo)` como se fosse um endereço cru e falha.
- **CONFIRMADA**: 2. O microfone nasce em TECLA, a tela não diz qual é a tecla, e o PTT só funciona com a janela do SEELE em foco e fora da caixa de texto.
  - LIDO: crates/seele-core/src/voice.rs:864-867 (padrão PushToTalk). crates/seele-core/src/preferences.rs:303 (`voice_mode` é Option, None num perfil novo). apps/seele-app/src/main.rs:1129-1131 só aplica o modo gravado. apps/seele-app/ui/tela-sessao.js:840-844 escreve «microfone abre na tecla» sem nomear a tecla, e 887-895 monta a linha de estado. tela-sessao.js:4291 `evento.code === teclaDeFalar && !digitando()`, com `keydown` em `window`. tela-sessao.js:4325-4328 fecha no `blur`. apps/seele-app/ui/base.js:107-110 (INPUT/TEXTAREA). A tecla só aparece na lista de CONFIGURAÇÕES (tela-server.js:1550-1556) e na ajuda `?` (index.html:3877). O botão de falar com o mouse saiu (index.html:1096-1106), então no modo TECLA a tecla é o único jeito de falar. O campo de mensagem recebe foco programaticamente (tela-sessao.js:2928 e 4171) e o mantém depois de enviar, então ESPAÇO digita espaço.
- **PARCIAL**: 3. Não há cancelamento de eco nem aviso de fone em tela nenhuma, então um notebook sem fone no modo VOZ produz eco para os outros.
  - LIDO e confirmado: não há AEC. crates/seele-audio/src/supressao.rs:4-6; crates/seele-audio/src/ganho.rs:14-17 cita `--features aec`, mas crates/seele-audio/Cargo.toml não tem seção [features]. O grep por VoiceProcessing/AEC/webrtc-audio/speex/eCommunications em crates/ e apps/ não achou implementação. A captura é cpal/HAL, sem a VPIO da Apple (INFERIDO). Também confirmado que nenhuma tela manda usar fone: só index.html:2727-2728 (OUVIR-ME) e camada-compartilhar.js:461-464 (eco na tela compartilhada). A exigência de fone está só no README.md:197-198 e no ADR 0007, este citado em index.html:2099. **Correção:** A consequência («produz eco para os outros») é INFERIDA e depende do modo. O padrão é TECLA, e nele o notebook só devolve som enquanto a pessoa segura a tecla. O eco contínuo aparece em VOZ/ABERTO. Nenhuma sessão com alto-falante foi medida. Há uma interação que o analista não diz: se o conserto da afirmação 2 for trocar o padrão para VOZ, o eco passa a ser o problema principal de toda máquina sem fone.
- **CONFIRMADA**: 4. Fechar a janela de quem hospeda encerra o servidor sem confirmação e sem mandar motivo; o convidado vê 5 minutos de «RECONECTANDO» e nada diz que quem hospeda desligou.
  - LIDO: apps/seele-app/src/main.rs:8119-8124 só trata `RunEvent::ExitRequested` e chama `despedir_se`, que em 8144-8153 faz só `connection.disconnect()`. O grep por CloseRequested/on_window_event/prevent_close/tray/beforeunload em src/ e ui/ volta vazio. O servidor roda no mesmo processo (main.rs:1560 `Hospedagem::iniciar`), então morre junto. crates/seele-server/src/lib.rs:886-891 («the close is abrupt»). O convidado só percebe depois de IDLE_TIMEOUT = 20 s (crates/seele-proto/src/transport.rs:33) e então entra na janela de 300 s (crates/seele-core/src/battery.rs:325). A faixa em index.html:730-750 não tem lugar para motivo. Contraste: tela-chamada.js:765-784 pergunta antes de SAIR DO SERVIDOR. **Correção:** É pior do que ele descreve. Mesmo que o servidor mandasse ServerShuttingDown, o cliente o classifica como recuperável (crates/seele-core/src/enlace.rs:5150-5156) e mostraria a mesma faixa de RECONECTANDO. A frase «O SERVIDOR ESTÁ ENCERRANDO» existe em apps/seele-app/ui/frases.js e não é alcançada nesse caminho. O conserto exige as duas pontas: difundir o motivo e dar a esse motivo um texto próprio na faixa.
- **CONFIRMADA**: 5. Os binários publicados não têm assinatura de código do sistema operacional.
  - LIDO: apps/seele-app/tauri.conf.json `signingIdentity: "-"`, `hardenedRuntime: false`. empacotar/macos.sh:113-117 (ad hoc, «não vale para o Gatekeeper»). empacotar/windows.ps1:16-19 e 205-220 só com TAURI_SIGNING_PRIVATE_KEY (minisign). grep por codesign/notarytool/signtool/AzureSign em empacotar/ só acha o release.yml, que é workflow_dispatch. MEDIDO: a página da v0.15.0 (curl no HTML do release) diz «Fora da integração contínua… macOS e Linux num Mac, Windows numa máquina Windows alcançada por SSH… commit e2fac4dab79870e69fd6a12c8e7961ec54a25d5e». Não rodei codesign nem Get-AuthenticodeSignature: exigiria baixar os binários, o que não está autorizado.
- **CONFIRMADA**: 6. O caminho de voz padrão da 0.15 (supressão ligada, limiar medido) não foi validado com voz humana.
  - LIDO: empacotar/notas/0.15.0.md, seção «Ativação por voz mais sensível»: «calibrado com sinal de teste, e ainda não com gravações de voz de gente». docs/review-v15-2026-09-21.md:14: «Não houve chamada entre duas máquinas físicas, teste acústico». A supressão está de fato no caminho vivo, não é só código existente: crates/seele-core/src/voice.rs:1833 cria `Supressao` na captura. **Correção:** A frase geral do README.md:288-290 («voz por microfone real entre duas máquinas» não verificada) está desatualizada. Há relatos de uso real entre máquinas em versões anteriores: ganho.rs:7-9; main.rs:8117 («se eu fecho o app no Mac, o usuário não sai da sala para o Windows»); tela-boot.js:736-739, com um teste de campo em que alguém falou 5 minutos para uma sala calada. O que nunca foi ouvido é o caminho novo da 0.15, não a voz entre máquinas em geral.
- **PARCIAL**: Bloqueante 1: o contorno do macOS ensinado no README «já não existe».
  - LIDO: README.md:124-130 ensina duas saídas: `xattr -dr com.apple.quarantine /Applications/SEELE.app` OU botão direito → Abrir. **Correção:** Só o botão direito → Abrir deixou de funcionar no macOS 15+ (conhecimento externo, não medido aqui). O `xattr` continua valendo, embora exija o Terminal, e Ajustes › Privacidade e Segurança › Abrir Mesmo Assim também funciona. O texto do README é conserto P, não bloqueio.
- **CONFIRMADA**: Existe: Quarto MORO/QUEM, em que o link antigo volta a achar o servidor pela impressão digital.
  - LIDO: apps/seele-app/src/main.rs:1006-1027 (`onde_mora_hoje`); crates/seele-server/src/alcance/encontro.rs:800-819 (MORO pelos dois sockets); crates/seele-server/src/alcance.rs:1020-1033 (o degrau 4 abre sempre que a máquina não tem IPv4 global, ou seja, todo anfitrião doméstico atrás de NAT, mesmo com UPnP). **Correção:** Só vale para links que carregam bilhete. Um servidor numa VPS com IPv4 global não põe bilhete, mas ali o IP é estável.
- **PARCIAL**: Existe (parcial): checagem da permissão do microfone com botão para os Ajustes, só em CONFIGURAÇÕES.
  - LIDO: crates/seele-audio/src/device.rs:1321-1331. No macOS, `consentimento_do_microfone` devolve `NaoSeSabe` sempre, e tela-server.js:1623-1636 esconde o bloco nesse caso. **Correção:** No macOS a checagem não existe nem em CONFIGURAÇÕES: só funciona no Windows. E ela saiu da entrada (tela-boot.js:742-747), embora o próprio comentário acima (736-739) registre o teste de campo em que alguém falou 5 minutos para uma sala calada.
- **CONFIRMADA**: Existe mas não está ligado: contagem de não lidas por canal.
  - LIDO: crates/seele-core/src/state.rs:741 `nao_lidas`. grep por nao_lidas/unread em crates/seele-ffi/src, apps/seele-app/src e ui/ sem consumidor. A própria casca, em tela-sessao.js:733-734, diz que «não há contagem de não-lidas… em lugar nenhum», um comentário desatualizado em relação ao core.
- **CONFIRMADA**: Ausente: notificação do sistema, som, contagem no ícone, bandeja.
  - LIDO: grep por Notification/new Audio/AudioContext/requestUserAttention/setBadge/tray em ui/*.js e src/*.rs sem uso real. apps/seele-app/ui/camada-portaria.js:456-458 registra que o plugin não está nas dependências.

### Severidades contestadas

- **O link seele:// não é clicável e não ensina nada a quem ainda não tem o SEELE**: o revisor propõe *importante*. Copiar e colar funciona hoje (camada-servidores.js:160-199), com impressão digital e versão no link, e o botão de copiar do anfitrião copia só o link (camada-portaria.js:752-754). É atrito, não prejuízo grave. A página https de convite é boa ideia e atende o pedido de «URL amigável», e a infraestrutura de páginas estáticas já existe (mods.seele.app.br em Cloudflare Pages, docs/notas-da-v0.12.0.md:89). Mas é escopo de produto, não pré-condição de uma 1.0 funcional. O conserto barato que deveria entrar é o campo aceitar um `seele://` no meio de texto colado (hoje exige startsWith, camada-servidores.js:168).
- **Quem hospeda fecha a janela e derruba todo mundo, sem pergunta e sem avisar os convidados**: o revisor propõe *importante*. A confirmação ao fechar é P e deve entrar, mas o dano é recuperável: o anfitrião reabre e hospeda de novo na mesma porta 8383 (PORTA_PADRAO, main.rs:1560), com o mesmo banco e a mesma impressão digital, e os convidados continuam tentando por 300 s (battery.rs:325). O reencontro sozinho é INFERIDO, não medido. O texto enganoso da faixa é real, mas é confusão, não perda. Se entrar, note que o conserto são duas pontas (servidor difunde, cliente trata ServerShuttingDown fora de enlace.rs:5150-5156).
- **Instalar exige vencer o sistema operacional: nada é assinado**: o revisor propõe *bloqueante_1_0 só para o Authenticode do Windows; importante para a notarização da Apple*. O Smart App Control impede a execução sem contorno, e o repositório registra isso num teste real (docs/assinatura-e-atualizacao.md:84-90): esse é o bloqueio. No macOS o `xattr` do README.md:127 e o Abrir Mesmo Assim ainda funcionam, e as novas perguntas de permissão a cada atualização (comentário do Info.plist) são incômodas, não graves. Corrigir o texto do README é P. A notarização exige hardenedRuntime=true e revisar Entitlements.plist, que é M.
- **Eco garantido para quem usa o alto-falante, e nenhuma tela manda usar fone**: o revisor propõe *importante (o aviso P entra na 1.0; o AEC não)*. Com o padrão TECLA (voice.rs:867), a exposição ao eco se limita a quem segura a tecla. O aviso de fone é P e deve entrar. O ponto que pesa na decisão: se a afirmação 2 for consertada trocando o padrão para VOZ, o eco sobe a bloqueante. A ordem certa é nomear a tecla na linha de estado primeiro, e só depois decidir o padrão, com a sessão de voz real.
- **Notificações do sistema (portaria, mensagem, entrada), tratadas pelo analista como M fora do corte**: o revisor propõe *importante, com prioridade acima do link clicável*. A portaria nasce ligada ao hospedar (main.rs:1573-1587, `semear_ligada`), e o único sinal de alguém batendo é a faixa dentro da janela (camada-portaria.js:436-458). Um anfitrião jogando com o SEELE em segundo plano deixa o amigo batendo a cada 15 s (tela-auth.js:112) sem saber. Na primeira noite isso vira um «não consigo entrar» mais provável do que o link não ser clicável.
- **Pacotes Mac Intel e Linux (marcados só como so_desenho)**: o revisor propõe *importante*. MEDIDO: os assets da v0.15.0 são só SEELE_0.15.0_aarch64.dmg e SEELE_0.15.0_x64-instalador.exe. Um amigo de Mac Intel não tem como usar sem compilar. E o README.md:119-121 ainda promete «.deb no Linux», contradizendo a própria página do release («Linux e Mac Intel ainda não têm pacote»).

### O que o analista não viu

- No macOS, microfone negado não aparece em lugar nenhum. crates/seele-audio/src/device.rs:1326-1331 devolve `NaoSeSabe` fora do Windows, tela-server.js:1623-1636 esconde o bloco, e a checagem saiu da entrada (tela-boot.js:742-747), embora o comentário ao lado (736-739) registre alguém falando 5 minutos para uma sala calada num teste de campo. Com assinatura ad hoc a permissão é pedida de novo a cada atualização (tela-server.js:1621-1622; Info.plist). É uma segunda causa do «ninguém me ouve» que o analista não viu, e é o caso típico de «o produto sabe e não conta».
- Mandar ServerShuttingDown não basta: enlace.rs:5150-5156 trata esse motivo como recuperável, e a faixa (index.html:730-750) não tem onde mostrar motivo. A frase «O SERVIDOR ESTÁ ENCERRANDO» (frases.js, junto de MOTIVOS) existe mas não é alcançada nesse caminho: existir não é funcionar.
- Ao fechar a janela, `Hospedagem::encerrar`, que desce a regra UPnP (hospedagem.rs:342-349), não roda. O `Drop` (hospedagem.rs:361-368) só chama `shutdown` e não desce a escada. A regra fica no roteador até a VALIDADE de 3600 s (alcance/porta.rs:86). LIDO no Drop; que os destrutores do estado do Tauri não rodam na saída do processo é INFERIDO. Menor, mas é a «sujeira que sobrevive ao programa» que o próprio comentário de alcance.rs:1122-1123 condena.
- A ajuda `?` diz ESPAÇO fixo (index.html:3877-3878), mesmo depois de a pessoa trocar a tecla. Só a lista de CONFIGURAÇÕES é reescrita (tela-server.js:1550-1556), e isso contradiz o próprio comentário da ajuda (index.html:3870-3872: «Uma ajuda que lista um atalho que o programa não tem é pior que ajuda nenhuma»).
- Um PTT global não pode usar ESPAÇO: um atalho global engole a tecla em todos os aplicativos. Então o PTT global implica trocar a tecla padrão, e no macOS pode exigir permissão de Monitoramento de Entrada, que a assinatura ad hoc faz ser pedida de novo a cada atualização (INFERIDO). O PTT global depende da assinatura.
- Chamada privada não se aproxima com o que existe: as permissões são globais por papel (crates/seele-server/src/permissions.rs:453-473; enum Permission em crates/seele-proto/src/control.rs:388-412, sem escopo por sala ou canal). Uma sala restrita exige modelo novo mais mudança de protocolo, e pela nota da 0.15 isso é mais um «atualizem juntos» (empacotar/notas/0.15.0.md, primeiro bloco).
- O atualizador só procura versão por clique (tela-server.js:1435; nenhuma chamada a `procurar_atualizacao` no arranque), enquanto cada subida de protocolo torna as versões incompatíveis. A frase Incompatible (frases.js:32-34) manda «atualize o SEELE nas duas máquinas» sem botão para isso. Numa 1.0 com amigos em versões diferentes, esse é o atrito recorrente depois do lançamento.
- O README está desatualizado em dois pontos de carga: promete «.deb no Linux» (README.md:119-121), que a página da v0.15.0 nega (MEDIDO), e diz que voz real entre duas máquinas não foi verificada (README.md:288-290), enquanto o próprio código registra relatos de campo entre Mac e Windows (main.rs:8117; ganho.rs:7-9).
- O instalador do Windows é por máquina (tauri.conf.json `nsis.installMode: perMachine`), o que exige elevação de administrador (UAC). Um amigo num PC gerenciado ou sem senha de administrador não instala. INFERIDO, menor.

### Desenho proposto pelo analista

## Princípio

Antes de feature nova, fechar as bordas da jornada, em ordem de «quem desiste primeiro». Quase tudo reaproveita o que já existe:
- enums de motivo (`ServerShuttingDown` já existe);
- `--entrar`;
- `permissao_de_microfone`;
- `Room::nao_lidas`;
- `procurar_atualizacao`;
- o caminho de assinatura do release.yml.

## Etapa 1 — «uma noite sem susto» (P, só casca e frases, sem protocolo)

1. **Primeira entrada numa sala de voz.** Uma camada curta pergunta «Como você quer falar?», com duas opções:
   - [SEGURAR ESPAÇO];
   - [QUANDO EU FALAR — use fone].

   A escolha é gravada com `set_voice_mode` e as preferências já existentes (main.rs:6003).
2. **A linha de estado nomeia a tecla.** Ela passa a dizer «segure ESPAÇO para falar», lendo `teclaDeFalar` (tela-sessao.js:3408). Com o foco no compositor, diz «clique fora da caixa para falar».
3. **Aviso de fone.** No modo VOZ ou ABERTO, uma faixa diz «sem fone, os outros ouvem o próprio eco» até a pessoa dispensar.
4. **Nome no primeiro uso.** Se `apelido_local` estiver vazio, abrir o perfil (camada-nomear.js:171) antes de CONECTAR ou HOSPEDAR.
5. **Microfone na sessão.** Chamar `permissao_de_microfone` ao entrar na sala e mostrar a frase de `PERMISSAO_DE_MICROFONE` (frases.js:199), com o botão para os Ajustes.
6. **Versões.**
   - Usar `ConviteLido.versao` e a versão local para dizer «o servidor está na X, você na Y», com [PROCURAR ATUALIZAÇÃO].
   - Chamar `procurar_atualizacao` ao abrir o app, só para avisar, sem instalar.
7. **Textos.**
   - O diálogo do link diz «quem usar vai bater à porta; você aprova aqui».
   - A tela de fim mostra o nome do servidor.
   - ENLACE vira CONVERSA.
   - Trocar a frase do furo de NAT depois de 1 teste de campo: fechar, reabrir o mesmo servidor, colar o link de ontem.
8. **Documentos.** Corrigir README.md:127-128 e 232, a ressalva do Windows na nota da 0.15, e o trecho do `publicar.sh` que gera «não traz nenhuma mudança de produto».

## Etapa 2 — quem hospeda fecha a janela (M)

1. **Confirmação ao fechar.** Com hospedagem ativa, `on_window_event(CloseRequested)` pergunta «Fechar derruba o servidor para N pessoas», com três saídas:
   - [MINIMIZAR];
   - [ENCERRAR E SAIR];
   - opcional: [CONTINUAR EM SEGUNDO PLANO], com bandeja pela feature `tray-icon` do Tauri.
2. **Despedida do servidor.**
   - Antes de `endpoint.close` (seele-server lib.rs:886), difundir `Disconnecting{ServerShuttingDown}` pelo barramento de eventos que as sessões já leem, e esperar cerca de 300 ms.
   - Chamar `Hospedagem::encerrar` em `ExitRequested` (main.rs:8119) e não só `connection.disconnect()`.
3. **Do lado do convidado.**
   - `MOTIVOS.ServerShuttingDown` passa a dizer «QUEM HOSPEDA DESLIGOU O SERVIDOR».
   - A faixa troca «CONEXÃO PERDIDA» por «servidor desligado — o SEELE entra sozinho se ele voltar em 5 min».

   `enlace.rs:5150` já trata esse motivo como bateria, então não há mudança de protocolo.

## Etapa 3 — avisos fora da janela (M)

1. **Notificações.** `tauri-plugin-notification`, passando pelo `cargo deny`, com três gatilhos:
   - batida na portaria (`atualizarPorta`, camada-portaria.js:~460);
   - mensagem nova com a janela sem foco;
   - alguém entrou na sua sala.
2. **Não lidas.** Expor `Room::nao_lidas` no Snapshot da FFI, desenhar o número por canal e somar no título da janela.
3. **Sons.** Sons curtos e locais de entrada e saída, com um interruptor.

## Etapa 4 — link que abre o app, e a «URL amigável» (M)

1. **Registrar `seele://`.**
   - `tauri-plugin-deep-link` mais `single-instance`.
   - No macOS, `CFBundleURLTypes`.
   - No Windows, `HKCU\Software\Classes\seele` gravado pelo instalador próprio (apps/seele-instalador).
   - Tudo desemboca no `--entrar` e em `cumprirAAbertura`.
   - Decidir o que a pendência #10 pede: **analisar e perguntar antes de conectar**, porque conectar já revela o IP do convidado ao servidor do link.
2. **Página https de convite**, por exemplo `https://seele.app.br/c#<convite>`.
   - É estática, e o convite fica no fragmento, que não chega ao servidor.
   - Oferece três botões: [ABRIR NO SEELE], [BAIXAR] (detectando o sistema e mostrando só pacotes que existem) e [COPIAR].
   - É o que o WhatsApp torna clicável, e responde ao pedido de URL amigável sem exigir DNS de ninguém. O campo NOME PÚBLICO continua para quem tem.

## Etapa 5 — assinatura (M, com custo)

- Apple Developer ID mais notarização: o Entitlements e o Info.plist já estão prontos.
- Azure Artifact Signing: release.yml:311-365 já está escrito.
- Duas saídas: religar o release.yml para as releases, ou portar essas duas etapas para `publicar.sh` e `windows.ps1`.
- Isso também acaba com as permissões esquecidas a cada atualização no macOS.

## Etapa 6 — homologar a voz com gente (M, 1 dia com 3 pessoas)

- Seguir `docs/teste-duas-maquinas.md` com Mac e Windows, cobrindo:
  - notebook sem fone no modo VOZ;
  - headset;
  - ventilador;
  - fala baixa.
- Medir eco, cortes, textura da supressão e latência.
- Registrar em `docs/m1-medicoes.md`.
- **Decidir o padrão TECLA ou VOZ e a força da supressão com base nessa medida.**

## Etapa 7 — depois, ou se a Etapa 6 mandar

- **AEC.**
  - macOS: `VoiceProcessingIO`.
  - Windows: o efeito AEC do modo comunicação.
  - Ou `webrtc-audio-processing`, revendo o ADR 0007.
- **PTT global** com uma tecla que não digita (F13, botão lateral do mouse).
- **Prazo em `device::open`.**
- **Tela entre dois Windows** (#33).
- **Pacotes Mac Intel/universal e .deb.**
- **Identidade e chave.** Exportar ou importar a identidade, ou deixar quem hospeda liberar um apelido; e um [ACEITAR A CHAVE NOVA] com a consequência escrita.
- **Volume por pessoa** guardado por apelido e reaplicado quando o ssrc muda.

## Dependências

- As Etapas 1, 2 e 3 são independentes entre si.
- A Etapa 4 fica melhor depois da 5: um esquema registrado por binário sem assinatura ainda passa pelo SmartScreen.
- A Etapa 6 deve anteceder a escolha final da Etapa 1.1, o padrão do microfone.

### Perguntas ao dono

- A 1.0 é só desktop? O specs/09-roadmap.md coloca o M6 (celular: ouvir, falar, ler) dentro da v1, e muitos amigos vão receber o link no celular.
- Você aceita pagar a Apple Developer (cerca de US$ 99/ano) e o Azure Artifact Signing (cerca de US$ 10/mês)? Sem isso, instalar continua exigindo Ajustes › Privacidade no Mac, e fica impossível em Windows com Smart App Control.
- Uma página https mínima de convite (por exemplo seele.app.br/c#…) contradiz o «sem serviço no meio»? O convite fica no fragmento e não chega ao servidor, mas a página vê o IP de quem abre.
- Qual deve ser o padrão do microfone na 1.0: TECLA (nunca dispara à toa, mas ‘ninguém me ouve’) ou VOZ (natural, mas com eco sem fone enquanto não houver AEC)? Ou uma pergunta no primeiro ingresso?
- Fechar a janela de quem hospeda deve continuar derrubando o servidor, ou o SEELE deve seguir hospedando na bandeja até ENCERRAR?
- Mac Intel e Linux entram na 1.0? Hoje só saem Apple Silicon e Windows x64, porque as releases são locais.
- Qual a política de protocolo na 1.0? Cada versão menor tem quebrado a conversa entre versões. Aceita congelar o protocolo com uma janela de compatibilidade N-1?
- Cancelamento de eco na 1.0: aceita rever o ADR 0007 (dependência nativa ou APIs do sistema), ou fica aviso de fone mais homologação e AEC depois?

---

## Outras opções

**Resumo do analista.** Fiz o ranking de 19 candidatos por valor para a 1.0 dividido pelo esforço, levando em conta o que já existe no código. Quatro coisas estão prontas no servidor e só falta ligá-las: a edição de mensagem, o desbanimento, a promoção a Operador e a menção. Para essas, basta um verbo no protocolo ou poucas linhas no cliente. Duas falhas atingem o usuário comum e custam pouco para corrigir: (1) o caminho documentado para ter um "servidor sempre ligado" instala a v0.10.0 e não tem pacote Linux (MEDIDO); (2) o padrão é aperte-para-falar, e ele só funciona com a janela em foco. Como hoje existe cancelamento de eco (AEC) em Rust puro (`aec3` 0.4.0 e `sonora` 0.2.0, MEDIDO no crates.io), cai o motivo do ADR 0007. O AEC vira o recurso de maior valor, com risco G e dependente de um spike. A decisão que define se isto é mesmo uma "1.0" é a política de protocolo: hoje qualquer subida tira do ar todas as versões anteriores. Retransmissão, mDNS, mobile, E2EE, reações, gravação, soundboard e música ficam fora. A 1.0 que recomendo tem 10 itens, na seção "proposta".

### O que existe

- `construido_e_publicado`: Supressão de ruído + limiar do portão medido pela sala (F01/F02) (crates/seele-core/src/voice.rs:1994; crates/seele-audio/src/supressao.rs; docs/adr/0055)
- `ausente`: Seam de compilação `--features aec` prometido pelo ADR 0007 (docs/adr/0007…:5,11; crates/seele-audio/src/ganho.rs:16-17 afirma que existe; crates/seele-audio/Cargo.toml não tem [features])
- `ausente`: Cancelamento de eco acústico (nenhum código; specs/03-audio.md:87-93 em aberto)
- `ausente`: Aviso na tela de que é preciso usar fone (só em README.md:197-198; nada em apps/seele-app/ui)
- `construido_e_publicado`: Aperte-para-falar com a janela em foco (padrão) (voice.rs:58,867; main.rs:1122-1130; tela-sessao.js:4324-4327 (blur fecha))
- `ausente`: Aperte-para-falar global (janela sem foco) (specs/03-audio.md:85 [EM ABERTO]; apps/seele-app/Cargo.toml:36 sem plugin)
- `construido_mas_nao_ligado`: Edição de mensagem (persistence/messages.rs:500; server.rs:55; session.rs:4798; state.rs:1213 — sem variante em ClientMessage (control.rs:1023-1571))
- `construido_e_publicado`: Responder e remover mensagem (autor remove a própria) (control.rs SendMessage.replies_to; session.rs:2478-2497)
- `construido_e_publicado`: Expulsar, banir (inclusive temporário), mover pessoa (control.rs:1164,1173; session.rs:2425,2440,2529; camada-moderar.js:817-847)
- `construido_mas_nao_ligado`: Desbanir (permissions.rs:553 sem chamador; camada-moderar.js:18-21)
- `construido_mas_nao_ligado`: Promover/rebaixar Operador (ManageRoles) (permissions.rs:41,113,423,436; único chamador session.rs:777-779 (observadores))
- `ausente`: Mudo forçado e registro de moderação (schema.rs:133-141 (bans com issued_by, sem log/tela))
- `so_desenho`: Menção (alerta «VOCÊ FOI CHAMADO») (control.rs:714; frases.js:121; o servidor nunca emite)
- `parcial`: Não lidas por canal (crates/seele-core/src/state.rs:349-355 (só na sessão))
- `ausente`: Notificação do sistema operacional (apps/seele-app/Cargo.toml:36-58 sem tauri-plugin-notification)
- `parcial`: Busca no histórico (crates/seele-core/src/search.rs; main.rs:7313 (só o que está carregado))
- `parcial`: Instalador de uma linha do servidor (install.sh:31 / install.ps1:28 apontam para DATA-AND-DEV/SEELE, latest = v0.10.0 (MEDIDO))
- `construido_mas_nao_ligado`: Pacote Linux do seeled/app (release.yml:141,221 sabe fazer; v0.15.0 e v0.14.2 sem nenhum asset Linux (MEDIDO))
- `parcial`: seeled como servidor sempre ligado (crates/seele-server/src/main.rs:35-37,60,88-99; sem escada (hospedagem.rs:95-97))
- `ausente`: Continuar hospedando com a janela fechada / bandeja (main.rs:8109-8124 só despede a sessão)
- `ausente`: Backup/exportação de servidor e identidade (nenhum comando; identity.rs:1-21 (arquivo sem exportação))
- `parcial`: Diagnóstico do furo de NAT para quem hospeda (alcance/encontro.rs:841 (só tracing); frases.js:618-621 (frase genérica))
- `ausente`: Retransmissão (degrau 5) (ADR 0022:125-130 fora por decisão; ADR 0045:108-109 permite via MOD, nenhum MOD faz)
- `parcial`: Compatibilidade de protocolo entre versões (version.rs:124-166; pendencias.md:6313 (#42); versoes.rs (launcher sem download, sem Windows))
- `construido_e_publicado`: Compartilhamento de tela (control.rs StartScreenShare; notas 0.15.0 — README.md:232 diz «não construído»)
- `ausente`: Descoberta na rede local (mDNS) (nenhum código)
- `ausente`: Mobile (M6) (nenhum código; specs/09-roadmap.md:97-103)
- `ausente`: E2EE de mídia e texto (specs/09-roadmap.md:109 (pós-v1))
- `ausente`: Capacidade de áudio para MODs (soundboard/música) (crates/seele-proto/src/mods.rs:88-112 — sem áudio)

### Lacunas

- **[BLOQUEANTE · esforço P · risco baixo] O servidor «numa linha» instala a v0.10.0, e no Linux não instala nada.** O caminho oficial para um servidor sempre ligado (README.md:132-143) baixa do repositório de código, cuja última release é a v0.10.0 (protocolo ≤3). Nenhum cliente 0.15 (protocolo 8) entra nesse servidor. No Linux, sistema típico de VPS, não existe pacote CLI nas duas últimas releases, e o script falha ao baixar. Quem seguir o README monta um servidor em que ninguém entra, e a mensagem que vê fala de versão ou de download, nunca do motivo real.
  - *Evidência:* install.sh:31, install.ps1:28 (REPO=DATA-AND-DEV/SEELE); MEDIDO: api.github.com/repos/DATA-AND-DEV/SEELE/releases/latest → v0.10.0 (2026-09-02); assets da v0.15.0 e da v0.14.2 sem nada Linux; install.sh:86 procura seele-cli-…-linux.tar.gz; release.yml:141,221 tem a matriz Linux
- **[BLOQUEANTE · esforço G · risco alto] A 1.0 não tem política de compatibilidade de protocolo.** Todo quadro sai carimbado com a versão global, e a janela de compatibilidade vale só para quem ouve. Qualquer 1.1 que acrescente uma variante tira do ar toda 1.0 instalada, nos dois sentidos, como a 0.15 fez com a 0.14. Uma «1.0» sem essa promessa não é 1.0. As lacunas de edição, desbanir e Operador exigem subir o protocolo de qualquer jeito, então a última subida pré-1.0 precisa juntar tudo e criar um mecanismo de extensão que um par antigo consiga pular.
  - *Evidência:* crates/seele-proto/src/version.rs:124-166; docs/pendencias.md:6313-6358 (#42); empacotar/notas/0.15.0.md:1-14; pendencias.md:76-78 (lado a lado não vale no Windows)
- **[importante · esforço M · risco medio] Falar exige a janela do SEELE em foco, e esse é o padrão.** O modo padrão é aperte-para-falar, e o blur da janela fecha o microfone. Quem está com um jogo ou outro app na frente não consegue falar sem trocar de janela, a menos que descubra a ativação por voz nas configurações. Tratar isso como gargalo de adoção para grupo de amigos é INFERIDO. Existe contorno (ativação por voz), então não é bloqueante.
  - *Evidência:* voice.rs:58 e :867 (PushToTalk padrão); main.rs:1122-1130; tela-sessao.js:4324-4327; specs/03-audio.md:85 [EM ABERTO]; apps/seele-app/Cargo.toml:36 sem plugin de atalho global
- **[importante · esforço P · risco baixo] O app nunca diz que é preciso fone.** Sem cancelamento de eco, quem usa alto-falante devolve a voz dos outros. O ADR 0007 aceitou isso com a condição de estar escrito de forma honesta, e só o README diz. Nem a tela MICROFONE E SOM nem a entrada numa sala avisam. Quem fala ouve o próprio eco sem saber por quê.
  - *Evidência:* README.md:197-198; docs/adr/0007…:7,9; grep em apps/seele-app/ui só acha eco do compartilhamento (camada-compartilhar.js:439,469)
- **[importante · esforço G · risco medio] Não há cancelamento de eco, e a razão de não haver caducou.** O ADR 0007 recusou AEC por ser dependência C/C++. Hoje existem portes de AEC3 em Rust puro (aec3 0.4.0 MIT/BSD e sonora-aec3 BSD-3), no mesmo espírito da supressão do ADR 0055. O ponto de inserção está pronto: captura e mistura correm no mesmo laço e em 48 kHz. Os crates ainda são 0.x, e o projeto nunca validou áudio de voz real nem para a supressão. Por isso isto pede um spike com medida (ERLE e CPU em notebook com alto-falante) antes de prometer.
  - *Evidência:* MEDIDO crates.io: aec3 0.4.0 (2026-09-16), sonora 0.2.0 (2026-07-29); voice.rs:1980 (quadro cru), :1994 (supressão), :2234 (mistura = referência); guarda textual voice.rs:2437-2445; specs/03-audio.md:89
- **[importante · esforço M · risco medio] Edição de mensagem construída e inalcançável.** Persistência, evento, difusão e aplicação no cliente existem e têm teste. Falta o verbo do cliente, o braço na sessão e o gesto na UI. Está no escopo da v1.0 da spec. Deve entrar junto na última subida de protocolo.
  - *Evidência:* persistence/messages.rs:500; server.rs:55; session.rs:4798; state.rs:1213; control.rs:1701; ClientMessage em control.rs:1023-1571 sem EditMessage; specs/00-visao-geral.md:21
- **[importante · esforço M · risco medio] Banimento sem volta e papel de Operador inalcançável.** Um banimento permanente dado por engano só se desfaz editando o SQLite na máquina de quem hospeda. Não há como promover alguém a Operador, então só o Comandante modera. Contorno parcial: o ban temporário (expires_at) existe. Precisa dos verbos Unban e SetRole na mesma subida, da lista de banidos e de uma superfície em #moderar.
  - *Evidência:* permissions.rs:553 (unban sem chamador), :423 (grant_role, só em session.rs:777-779); camada-moderar.js:18-21; schema.rs:133-141
- **[importante · esforço M · risco baixo] Menção desenhada e nunca emitida; sem notificação do sistema.** AlertReason::Mentioned e a frase «VOCÊ FOI CHAMADO» existem e ninguém os dispara. Com a janela minimizada, uma mensagem para você passa despercebida. Dá para fazer sem mexer no protocolo: o núcleo detecta @apelido (a dobra de acento de search.rs já existe) e a casca chama a notificação do SO. Isso exige uma dependência Tauri nova, a julgar pelo ADR 0020.
  - *Evidência:* control.rs:714; frases.js:121; grep em crates/seele-server/src sem emissor; state.rs:349-355; apps/seele-app/Cargo.toml:36-58
- **[importante · esforço M · risco baixo] seeled não serve para servidor sempre ligado em casa, e dá instruções falsas.** O daemon não pede porta ao roteador nem registra no ponto de encontro (quarto), então atrás de NAT/CGNAT só serve à rede local. Ao subir, ele manda rodar `connection`, que não é distribuído, e não imprime um link seele://. O nome é fixo em «Casa». Faltam subcomandos de papel e desbanimento. E o primeiro a conectar vira Comandante, o que é um risco num servidor aberto numa VPS.
  - *Evidência:* crates/seele-server/src/hospedagem.rs:95-97; crates/seele-server/src/main.rs:35-37,60,88-99,117-124; release.yml:653-658 (pacote só com seeled); README.md:188,220; permissions.rs:330-370
- **[importante · esforço M · risco medio] Fechar a janela derruba o servidor hospedado sem perguntar.** Quem hospeda pelo app e fecha a janela encerra a conversa de todos. A saída é limpa, mas não há confirmação, bandeja ou opção de continuar hospedando. Pode ser coberto pela frente de persistência; se não for, é a forma barata de ter um servidor sempre ligado sem terminal.
  - *Evidência:* apps/seele-app/src/main.rs:8109-8124; crates/seele-server/src/hospedagem.rs:13-18
- **[importante · esforço M · risco baixo] O furo de NAT que não abre é invisível para os dois lados.** Quem hospeda recebe o aviso de que alguém com o link está chegando, mas isso só vai para o log. Se o aperto de mão não vem depois, ninguém fica sabendo. Quem tenta entrar lê «confira o endereço e a porta UDP», o que manda procurar o defeito no lugar errado. Esse diagnóstico, que fica na própria máquina e não manda dado a ninguém, é também o único jeito de saber se o degrau 5 vale o custo.
  - *Evidência:* crates/seele-server/src/alcance/encontro.rs:841; frases.js:618-621; encontro.rs:32-37
- **[importante · esforço P · risco baixo] README e specs 00/09 descrevem um produto que não existe mais.** O README ensina `connection` e diz que o compartilhamento de tela não foi construído. A spec 00 põe a TUI como referência e item da v1.0. Sem reescrever a spec 00 (a fonte de verdade), a 1.0 não tem critério de aceite, e o usuário que segue o README tropeça logo no primeiro comando.
  - *Evidência:* README.md:176-191,220,232; specs/00-visao-geral.md:9-13,26; docs/adr/0039…:1-10
- **[importante · esforço M · risco baixo] Sem backup/exportação de servidor e de identidade.** Um servidor é um SQLite (identidade TLS, contas, histórico, MODs) numa pasta de configuração, sem comando para salvar ou restaurar. A identidade da pessoa é um arquivo sem exportação: um PC novo vira pessoa nova, e o apelido e o papel ficam presos à chave antiga. Exportar o servidor preserva a impressão digital, portanto o `fp=` dos links antigos. É o que habilita «migrar para uma VPS sem trocar de link»; ligar isso ao quarto é da frente de persistência.
  - *Evidência:* crates/seele-server/src/persistence/instancia.rs:22,119 (backup citado só em comentário); crates/seele-core/src/identity.rs:1-21; apps/seele-app/src/main.rs:1541-1560
- **[desejável · esforço P · risco baixo] O ADR 0007 e o ganho.rs afirmam um seam que não existe.** O custo de reverter do ADR 0007 é «baixo, desde que o seam exista»; ele não existe, e o ganho.rs diz que «continua de pé». A spec 03:89 diz que não há AEC em Rust puro. Tudo isso deve ser corrigido no ADR que substituir o 0007.
  - *Evidência:* docs/adr/0007…:5,11; crates/seele-audio/src/ganho.rs:16-17; crates/seele-audio/Cargo.toml (seções nas linhas 1,11,58,85,91,97,101); specs/03-audio.md:89
- **[desejável · esforço M · risco baixo] Busca só no que está carregado.** A busca dobra acento e é boa, mas não alcança histórico que não foi baixado. Busca no servidor exige FTS5 e um verbo novo; é para a 1.1, via envelope de extensão.
  - *Evidência:* crates/seele-core/src/search.rs:1-40; apps/seele-app/src/main.rs:7313
- **[desejável · esforço M · risco baixo] Sem mudo forçado nem registro de moderação.** Operador não cala ninguém; o histórico de quem baniu quem só existe como coluna, sem tela.
  - *Evidência:* control.rs (sem variante de mudo forçado); schema.rs:133-141
- **[desejável · esforço P · risco baixo] Mobile sem decisão formal de corte.** As specs 00 e 09 ainda preveem o M6 (consumo) antes do pós-v1, e não há uma linha de código. Precisa de decisão escrita tirando o mobile da 1.0.
  - *Evidência:* specs/00-visao-geral.md:36; specs/09-roadmap.md:97-103; nenhum código android/ios
- **[desejável · esforço M · risco baixo] Sem descoberta na rede local.** Não há mDNS. O valor é baixo porque o convite já leva o endereço de rede local. Anunciar «há um SEELE aqui» no Wi-Fi de um café contraria a discrição do produto, então, se entrar, entra como opt-in.
  - *Evidência:* grep mdns/bonjour/zeroconf vazio; ADR 0037

### O que o revisor conferiu

- **PARCIAL**: 1. O motivo do ADR 0007 (dependência C/C++) caiu: há AEC3 em Rust puro, e o pipeline tem captura e mistura no mesmo laço, com a referência à mão.
  - MEDIDO (API do crates.io): aec3 0.4.0 publicado em 2026-09-16, MIT OR BSD-3, 36k downloads. As dependências normais são crossbeam-channel, log, num-complex e rustfft, sem nenhum -sys. sonora-aec3 0.2.0 (2026-07-29, BSD-3) depende só de sonora-common-audio, sonora-fft e sonora-simd. LIDO: há um único laço, voice.rs:1872 `while !controls.stop`. Nele a captura é drenada em :1970, a supressão roda em :1994 e a mistura em :2231 (`mixer.mix(&borrowed, &mut mixed)`). A referência está disponível. LIDO: nenhum Cargo.toml do workspace tem a feature `aec` (grep vazio), e ganho.rs:16-17 afirma que «o seam de `--features aec` continua de pé». **Correção:** Cai apenas o argumento da dependência C/C++. Três coisas ficaram sem medida. (a) Maturidade: o aec3 0.4.0 tem 6 dias de vida. (b) Atraso variável entre referência e eco: `mixed` sai antes da reamostragem e do anel de saída, e o compasso injeta silêncio de preparo (voice.rs ~:2155-2160, `prime_samples`). O estimador de atraso do AEC3 teria de acompanhar isso, e ninguém mediu. (c) ERLE e CPU por quadro. O ADR 0007:13 diz que reverter é barato «desde que o seam exista», e o seam não existe. Pelos critérios do próprio ADR, então, o custo não é baixo. O ponto de inserção, porém, é óbvio (entre :1980 e :1994).
- **CONFIRMADA**: 2. O padrão é aperte-para-falar, que só funciona com a janela em foco; não há atalho global.
  - LIDO: voice.rs:58-60 (`PushToTalk` «is the default») e :867 (`mode: AtomicU8::new(VoiceMode::PushToTalk...)`). main.rs:1126-1130 só troca o modo se houver preferência gravada. preferences.rs:170 cria `voice_mode: None`. tela-sessao.js:4324-4327: o `blur` fecha a fala. O grep por global-shortcut/globalShortcut/RegisterHotKey/CGEventTap/rdev em apps e crates não achou nada. apps/seele-app/Cargo.toml:36 declara `tauri = { features = [] }` e nenhum plugin de atalho. specs/03-audio.md:85 continua [EM ABERTO].
- **PARCIAL**: 3. Toda subida de protocolo tira do ar todas as versões anteriores, nos dois sentidos. Por isso a 1.0 precisa de uma última subida que junte as variantes pendentes e de um mecanismo de extensão.
  - LIDO: control.rs:2301 carimba `PROTOCOL_VERSION`, a versão global. control.rs:2326 chama `negotiate` antes de ler o corpo. version.rs:124 fixa `PROTOCOL_VERSION = 8`, e version.rs:166 fixa `COMPATIBILITY_WINDOW = 1`. No commit da v0.14.2 (0018e71), `git show 0018e71:crates/seele-proto/src/version.rs` dá `PROTOCOL_VERSION: u8 = 7` e janela 1, com o mesmo `decode`. Portanto o cliente 0.14.2 recusa qualquer quadro com carimbo 8, e o fato em si está confirmado. MAS: (a) o servidor já cala variantes por versão do par em session.rs:4177-4220 (`entende_a_mensagem`), então metade do «mecanismo de extensão» existe e falta o carimbo negociado em control.rs:2301 e o portão simétrico no cliente; (b) desbanir e promover a Operador NÃO exigem subir o protocolo quando quem age é quem hospeda, porque o app já tem comandos que escrevem direto no banco hospedado (main.rs:7171-7176 `ligar_portaria`, main.rs:7221-7229 `revogar_admissao`, via `persistence_hospedada`) e o `seeled` já tem subcomandos locais (crates/seele-server/src/main.rs:35-36 `convite`, `senha`); (c) a quebra é anunciada com clareza (frases.js:32-34: «VERSÃO INCOMPATÍVEL… atualize o SEELE nas duas máquinas»), e existe atualizador assinado (tauri.conf.json:57-66). **Correção:** O fato técnico está certo. Está errada a inferência de que «as lacunas de edição, desbanir e Operador exigem subir o protocolo de qualquer jeito»: só a edição, e a moderação feita por quem não hospeda, exigem. Também está superestimado o esforço do mecanismo, porque o portão por variante do lado do servidor já existe.
- **CONFIRMADA**: 4. O caminho documentado de servidor sempre ligado instala a v0.10.0 e não tem pacote Linux.
  - LIDO: install.sh:31 e install.ps1:28 usam `DATA-AND-DEV/SEELE`. MEDIDO sem a API, que estava sem cota: `curl -w %{redirect_url} https://github.com/DATA-AND-DEV/SEELE/releases/latest` redireciona para `/tag/v0.10.0`, e o mesmo em SEELE-RELEASES redireciona para `/tag/v0.15.0`. HEAD em SEELE/releases/download/v0.10.0/seele-cli-0.10.0-macos.tar.gz respondeu 200, e o arquivo linux da mesma versão respondeu 404. O SHA256SUMS da v0.10.0 lista seele-cli-0.10.0-windows-x86_64.zip. Os SHA256SUMS de SEELE-RELEASES v0.12.1, v0.13.0, v0.14.0, v0.14.1, v0.14.2 e v0.15.0 não têm NENHUM arquivo Linux e trazem só SEELE_*_aarch64.dmg para o Mac. O latest.json da v0.15.0 só tem darwin-aarch64 e windows-x86_64. install.raw respondeu 200, então o repositório é público. **Correção:** A falha é mais ampla do que o analista disse. No macOS e no Windows o script instala, sem avisar, o seeled 0.10.0 (protocolo 3), e não só falha no Linux. `SEELE_VERSION=v0.15.0` também não salva, porque monta a URL em DATA-AND-DEV/SEELE e recebe 404. Nenhuma das seis últimas releases teve pacote Linux (a lacuna não se limita às duas últimas). O README:118-120 promete `.deb` no Linux. Não há app para Mac Intel nem atualização para ele.
- **CONFIRMADA**: 5. Edição de mensagem, desbanir e promover a Operador estão prontos no servidor e não têm verbo no protocolo nem chamador.
  - LIDO: Messages::edit (persistence/messages.rs:500) só é chamado em testes (messages.rs:843, :1015-1016). Ninguém no servidor emite `Event::MessageEdited`: o grep só acha os `match` em session.rs:4098 e :4798 e despacho.rs:496. O cliente já aplica e desenha a edição (state.rs:1213-1222; tela-sessao.js:1206, «editada»). `unban` (permissions.rs:553) só é chamado em teste (permissions.rs:1023). `grant_role` fora de testes só aparece em session.rs:779, com OBSERVER_ROLE. As variantes de ClientMessage (control.rs:1025-1595) não têm Edit, Unban nem SetRole. O seeled só tem os subcomandos `convite` e `senha` (main.rs:35-36). camada-moderar.js:18-21 admite que «`unban` não é verbo do protocolo». **Correção:** Um ajuste ao resumo: ele diz que a «menção» está «pronta no servidor», mas o próprio analista a classifica como so_desenho. Só existem `AlertReason::Mentioned` (control.rs:714) e a frase (frases.js:121). Não há detecção de @nome nem no servidor nem no cliente.
- **PARCIAL**: 6. Quem hospeda sabe quando alguém tenta entrar pelo furo de NAT, mas o produto só grava isso no log e não cruza com o aperto de mão; por isso o degrau 5 não pode ser decidido com dado.
  - LIDO: encontro.rs:840-841 emite só `tracing::info!(... "degrau 4: alguém com o link está chegando; furando")`. O grep por furo/furar em seele-ffi, apps/seele-app/src e ui não acha contador nem evento. frases.js:620-623 mostra `SemResposta` genérico. MAS o filtro padrão do app é `seele_server=info` (main.rs:7810-7814) e escreve em `seele.log` na pasta de configuração (main.rs:7719). Portanto a linha fica gravada no disco de quem hospeda. **Correção:** O produto não expõe nem correlaciona o dado, e isso se confirma: é o «sabe e não conta». Mas o dado existe no seele.log de cada anfitrião, e dá para decidir o degrau 5 recolhendo logs de campo. «Não pode ser decidido com dado» exagera. O correto é dizer que ele não pode ser decidido pelo produto sozinho.

### Severidades contestadas

- **A 1.0 não tem política de compatibilidade de protocolo**: o revisor propõe *importante*. Nenhum usuário da 1.0 sai gravemente prejudicado no dia da 1.0: o dano aparece na 1.1 e se limita a obrigar todos a atualizar ao mesmo tempo. Três coisas já o mitigam. A recusa é explícita e manda atualizar (frases.js:32-34). O atualizador tem manifesto assinado (tauri.conf.json:57-66). O link carrega `v=` e abre a versão do servidor quando ela está instalada (main.rs:6929-6936). Além disso, o argumento de que a subida é obrigatória por causa de desbanir/Operador está refutado, porque os dois cabem como comandos de quem hospeda, sem protocolo (padrão main.rs:7171-7229). A recomendação de juntar variantes numa última subida antes da 1.0 continua boa. Só deixa de ser bloqueio, a menos que o dono queira prometer estabilidade de fio na 1.0, e aí a decisão é de produto, não uma lacuna.
- **O servidor «numa linha» instala a v0.10.0, e no Linux não instala nada**: o revisor propõe *bloqueante_1_0 só na parte do instalador (é trivial: trocar REPO para SEELE-RELEASES em install.sh:31 e install.ps1:28). A parte do Linux passa a importante, desde que o README deixe de prometer `.deb`.*. O dano grave é o instalador entregar, sem aviso, um servidor de protocolo 3 no macOS e no Windows (MEDIDO), e ele é mais amplo do que o analista descreveu. Já a ausência de Linux vem de sempre: nenhuma release desde a v0.12.1 teve Linux (MEDIDO). Isso bloqueia a 1.0 só se a 1.0 prometer Linux. Hoje o README:118-120 promete, então o mínimo é publicar o pacote ou tirar a promessa.
- **Cancelamento de eco acústico como «recurso de maior valor»**: o revisor propõe *importante (condicionado ao modo padrão)*. Com aperte-para-falar como padrão (voice.rs:867), o eco só acontece enquanto a pessoa local segura a tecla e outro fala ao mesmo tempo. O valor do AEC só vira máximo se o padrão mudar para ativação por voz, que é justamente a saída barata para o problema do foco (afirmação 2). As duas recomendações estão acopladas e o ranking as trata como independentes. Se vier um PTT global, a urgência do AEC cai. Se o padrão virar ativação por voz, o AEC, ou no mínimo o aviso de fone na tela (hoje ausente), passa a ser pré-requisito.

### O que o analista não viu

- As notas da v0.15.0 prometem o que o app não faz. empacotar/notas/0.15.0.md:11-14 diz que «Em CONFIGURAÇÕES dá para baixar a v0.14.2 e guardá-la ao lado desta». O código só baixa «a mais nova»: versoes.rs:354 `manifesto.mais_nova()` e camada-versoes.js:110-119 `baixarAMaisNova`. O latest.json da v0.15.0 (MEDIDO) tem as chaves `version, notes, pub_date, platforms` com uma única versão, a 0.15.0. Quem está na 0.15.0 e não baixou a 0.14.2 antes não tem como obtê-la pelo app, e no Windows versão lado a lado nem funciona (pendencias.md:76-78). Quando a versão do link não está instalada, `analisar_convite` (main.rs:6929-6936) devolve `pode_abrir_naquela_versao=false` e segue direto para a recusa Incompatible.
- O banimento é contornável num servidor aberto, e aberto é o padrão. O ban é por person_id (permissions.rs:569-575). A identidade é uma chave Ed25519 local, sem conta (identity.rs:1-21). O servidor fica aberto enquanto não houver senha nem convite (admissao.rs:106-108), e a portaria nasce desligada (portaria.rs:118-128; main.rs:7095). INFERIDO: quem foi banido apaga o arquivo da chave e volta como outra pessoa. Isso pesa na «call privada» que o dono pediu e no valor de desbanir/Operador.
- O PTT global não custa «poucas linhas». O tauri-plugin-global-shortcut usa hotkey registrado (RegisterHotKey no Windows, hotkey do sistema no macOS), que consome a tecla no sistema inteiro: um PTT em Espaço quebraria a digitação em qualquer outro app. PTT de verdade precisa de gancho de baixo nível e de permissão de acessibilidade no macOS, como specs/03-audio.md:85 já registra. A alternativa de custo mínimo para a 1.0 é trocar o padrão no primeiro uso, com o aviso de fone.
- Há um defeito de retorno de voz fora do microfone, que o AEC não resolve. No Windows, compartilhar a tela inteira com som leva a conversa junto (notas 0.15.0.md:16-27; ADR 0055:269-276 remete ao ADR 0054). É captura de loopback, não passa pelo microfone. O analista trata o eco como um problema só.
- O README está defasado desde 2026-08-31 (`git log -1 -- README.md` → aa77ad8). O :232 diz que o compartilhamento de tela está «ainda não construído», o :118-120 promete `.deb`, o :133-143 aponta o instalador para o repositório errado e o :288-290 diz que a voz por microfone real entre duas máquinas «ainda não verificado». Não há outro registro de que essa validação tenha sido feita: o grep em pendencias.md e teste-duas-maquinas.md não achou nada. Para uma 1.0, a porta de entrada contradiz o produto em quatro pontos, e o analista só viu o :232.
- Não há app para Mac Intel nem atualização para ele (MEDIDO: todas as releases de v0.12.1 a v0.15.0 só têm `SEELE_*_aarch64.dmg`, e o latest.json só tem `darwin-aarch64`). O `baixar_versao` devolveria `SemPacoteParaEsteSistema` (versoes.rs:362-365).
- As quatro funcionalidades que o dono pediu não aparecem na lista do analista, nem como existentes nem como candidatas. Persistência de link: parcial e ligada. Há servidores.json (ecf0f0b), e o QUEM é perguntado de verdade no caminho de conexão (seele-ffi/src/lib.rs:7797 `onde_mora_hoje`, chamado em main.rs:1012). URL amigável: só com DNS próprio da pessoa (c42a30f, campo opcional). Multiconexão: recusada em main.rs:899-900 (`AlreadyConnected`), ADR 0031 proposto. Chamada privada: nada. Se outra frente cobre esses itens, ok. Numa frente de escopo da 1.0, porém, é a omissão de maior peso.

### Desenho proposto pelo analista

## Princípio
A 1.0 não precisa de funções grandes novas. Precisa **ligar o que já está pronto**, **consertar dois caminhos que enganam quem os segue** e **fazer uma promessa de compatibilidade**. Os únicos investimentos grandes que recomendo são o protocolo (G, obrigatório) e o cancelamento de eco (G, condicionado a um spike).

## Ordem de trabalho (dependências explícitas)

### Etapa 0: escrever o que é a 1.0 (P, antes de tudo)
- Reescrever `specs/00-visao-geral.md`: escopo sem TUI (ADR 0039), com tela compartilhada, mobile explicitamente fora e critérios de aceite da 1.0.
- Ajustar o `specs/09-roadmap.md`: o M6 passa para depois da 1.0.
- Corrigir o README: tirar `connection` (`:188,220`) e trocar o «não construído» da tela (`:232`).
- Sem isso, «a 1.0 está pronta» não tem contra o que ser conferida.

### Etapa 1: consertos baratos que param de enganar (P, independentes, podem sair numa 0.15.x)
1. **Instaladores**: `install.sh:31` e `install.ps1:28` passam a apontar para `DATA-AND-DEV/SEELE-RELEASES`, e a próxima publicação não pula o Linux (`release.yml:141,221` já sabe fazer). Guarda contra regressão: um teste do `install.sh` com `SEELE_BASE` falso que confira o repositório e o nome do pacote. Prova de reversão: voltar o REPO e ver o teste reprovar.
2. **`seeled` fala a verdade**:
   - trocar as linhas `connection --server` (`main.rs:88-99`) por um `seele://` com `fp=`, reusando o que o `criar_convite` já monta;
   - aceitar `--nome` no lugar do literal `"Casa"` (`:60`);
   - acrescentar os subcomandos locais `seeled desbanir <apelido>` e `seeled papel <apelido> operador` sobre `Permissions::unban`/`grant_role` (`permissions.rs:423,553`). Isso roda contra o banco e **não exige protocolo**;
   - publicar em `docs/` uma unidade systemd de exemplo.
3. **Aviso de fone**: uma linha fixa em CONFIGURAÇÕES · MICROFONE E SOM e uma na primeira entrada numa sala de voz. A frase mora em `frases.js`, e um guarda do tipo do `the_nat_punching_rung_names_its_cost…` cobra que ela exista enquanto não houver AEC.

### Etapa 2: a última subida de protocolo, ou «protocolo da 1.0» (G, bloqueante)
Precisa de um ADR próprio que feche a pendência 42. O desenho (INFERIDO, a validar):
- **Variantes novas, uma única vez, no fim das listas**:
  - `ClientMessage::EditMessage { message, body }`, cujo braço chama `Messages::edit` (`messages.rs:500`) e emite o `Event::MessageEdited` que já é difundido (`session.rs:4798`);
  - `UnbanPerson { person }`;
  - `SetRole { person, role, on }`, restrito a Operador/Pessoa/Observador e exigindo `ManageRoles`;
  - `ListBans`;
  - tudo o que as frentes de multiconexão e permanência precisarem no fio.
- **O envelope de extensão**: `ClientMessage::Extensao { tipo: u16, corpo: Vec<u8> }` e o espelho em `ServerMessage`. O `Hello` declara os `tipo` que o cliente entende; o servidor só envia os declarados, generalizando `session::entende_a_mensagem` de versão para conjunto. Quem recebe um `tipo` desconhecido ignora. A regra da 1.x passa a ser: **função nova entra como `Extensao`, e `PROTOCOL_VERSION` só sobe na 2.0**.
- **Guarda contra regressão**: um teste que decodifica um fluxo com `Extensao` de tipo desconhecido seguido de uma variante conhecida e confere que a segunda chega inteira. Prova de reversão: tirar o braço de ignorar e ver o teste reprovar.
- **UI**: edição com o gesto de responder (`tela-sessao.js`); DESBANIR, lista de banidos e PROMOVER/REBAIXAR em `camada-moderar.js`, passando pelo `armarAto` que já existe.

### Etapa 3: falar sem foco (M, independente da etapa 2)
- Usar o `tauri-plugin-global-shortcut`, que tem estado Pressed/Released. Isso é INFERIDO; conferir num spike de meio dia se Windows e macOS entregam a soltura.
- O evento chama o mesmo `segurarFala`. O `blur` (`tela-sessao.js:4324`) só fecha o microfone quando a tecla global **não** está ativa.
- A preferência é separada da tecla local (`main.rs:6262-6290`).
- Custos a dizer na tela: a tecla registrada é «tomada» dos outros apps, e o Wayland não suporta.
- Alternativa: hook de baixo nível sem tomar a tecla, que pede permissão de Acessibilidade no macOS. É decisão do dono.

### Etapa 4: eco (spike M, depois G ou nada)
- **Spike de 3 dias**:
  - inserir `aec3` 0.4 (ou `sonora-aec3`) entre `voice.rs:1980` e `:1994`, isto é, antes da supressão;
  - a referência de renderização é `mixed` (`voice.rs:2234`), em subquadros de 480 amostras;
  - medir o ERLE com alto-falante e microfone embutidos de um MacBook e de um notebook Windows, e o custo de CPU por quadro;
  - criar `tests/aec_e_supressao.rs` no molde de `supressao_e_portao.rs`: eco sintético (atraso de 30–120 ms mais filtro) que reprove sem o AEC;
  - atualizar o guarda textual `voice.rs:2437-2445` para a ordem nova: captura → AEC → supressão → portão → ganho.
- **Critério para entrar na 1.0** (a definir pelo dono; sugestão): ≥ 20 dB de ERLE medido em hardware real, sem regressão de latência além de 10 ms e sem erro de tamanho de quadro, a lição do ADR 0055.
- **Se passar**: um ADR substitui o 0007, a spec 03:87-93 é corrigida, o `ganho.rs:16-17` também, e a ativação por voz pode virar o padrão. Isso resolve metade da etapa 3 para quem não usa fone.
- **Se não passar**: a 1.0 sai com o aviso da etapa 1, e o AEC vai para a 1.1.

### Etapa 5: texto que chama (M, sem protocolo)
- O núcleo detecta `@apelido` com a dobra de `search.rs` em `MessageReceived` de canal não aberto ou com a janela sem foco, e produz o `AlertReason::Mentioned` que já existe (`control.rs:714`, frase em `frases.js:121`).
- A casca chama o `tauri-plugin-notification`, via adendo ao ADR 0020.
- As não lidas continuam valendo só na sessão (`state.rs:349-355`) na 1.0.

### Etapa 6: alcance que se explica (M)
- **Anfitrião**: em `atender` (`encontro.rs:781-845`), guardar `(endereço, instante)` de cada aviso. Quando um aperto de mão daquele IP completa, apagar. Depois de ~10 s sem aperto de mão, emitir um evento pela FFI para a casca: «ALGUÉM COM O SEU LINK TENTOU ENTRAR E O CAMINHO NÃO ABRIU». O metadado não sai da máquina.
- **Convidado**: quando o convite só tinha o candidato `enc=` e tudo morreu, mostrar uma frase própria no lugar de `SemResposta` (`frases.js:618-621`), nomeando «as duas redes recusam o furo» e as saídas reais: porta encaminhada, `seeled` numa VPS ou VPN.
- É o que produz, na mão de quem usa, o dado para decidir o degrau 5.

### Etapa 7: servidor sempre ligado sem terminal (M–G; só se a frente de persistência não cobrir)
- O `seeled` sobe a mesma escada do app, reusando `Escada::subir` como em `hospedagem.rs:115-121`, com `--sem-escada` para VPS.
- No app, `CloseRequested` hospedando mostra «fechar derruba N pessoas» e oferece continuar na bandeja (feature `tray-icon` do Tauri) e um ENCERRAR SERVIDOR explícito.

## Sobre a retransmissão (degrau 5)
Fica fora da 1.0 por três razões:
1. O custo é de banda recorrente, e para tela compartilhada é alto.
2. Quem tem uma VPS ganha mais rodando o `seeled` nela (degrau 1, um salto a menos). A exceção é quem faz questão de que o **conteúdo fique em casa**: um relé na VPS só veria QUIC cifrado, porque o TLS termina em quem hospeda. É a única versão «auto-hospedada» do degrau 5 que preserva a filosofia, e fica para a 1.2 se a etapa 6 mostrar que o caso é frequente.
3. O ADR 0045 já deixa um MOD abrir túnel de terceiro, com aviso.

## Recomendação de escopo

### O que é a 1.0 (10 itens)
1. **Protocolo da 1.0**: a última subida com envelope de extensão e a política «1.x conversa com 1.x» escrita (etapa 2).
2. **O link que volta a funcionar amanhã**, conforme a frente de persistência. O quarto já está no ar; falta fechar o que ela listar e corrigir a frase `FuroDeNat` (`frases.js:767-770`).
3. **Servidor sempre ligado que funciona**: instaladores e pacote Linux, `seeled` com link e nome, desbanir e papel pela CLI e unidade systemd (etapa 1.1–1.2). Se a frente de persistência não cobrir, também a escada no `seeled` e a bandeja no app (etapa 7).
4. **Falar sem a janela em foco** (etapa 3).
5. **Eco**: aviso na tela já; AEC em Rust puro se o spike passar (etapas 1.3 e 4).
6. **Edição de mensagem** (etapa 2).
7. **Moderação reversível**: desbanir, promover/rebaixar Operador, lista de banidos (etapa 2).
8. **Menção e notificação do sistema** (etapa 5).
9. **Alcance que se explica**: o anfitrião vê a tentativa que não entrou, e o convidado lê a causa certa (etapa 6).
10. **Casa em ordem para uma 1.0**: spec 00/09 e README reescritos (etapa 0), licença definida (o repositório de código é público, MEDIDO, e o README diz «ainda não definida») e CI rodando (outra frente).

### 1.1
- AEC, se ficou fora da 1.0.
- Multiconexão (ADR 0031, proposto, nada construído) e chamada privada, conforme as frentes próprias.
- Exportar e importar servidor e identidade, e migrar de máquina mantendo o `fp`.
- Busca no histórico inteiro (FTS5 via `Extensao`).
- Mudo forçado e registro de moderação.
- Não lidas persistentes.
- Launcher que baixa versões e funciona no Windows (ADR 0046).

### 1.2
- mDNS opt-in.
- Relé próprio numa VPS («conteúdo fica em casa»), se o dado da etapa 6 justificar.
- Fixar mensagem.

### Fora, com motivo
- **Mobile**: GG, nada existe; é decisão formal para a 2.0 com o M6.
- **E2EE**: quem hospeda é a âncora de confiança (`specs/09:109`).
- **Reações e threads**: não-objetivo (`specs/00:35`); caminho de MOD via `contribuicoes`.
- **Gravação, soundboard e música**: gravação é pós-v1, bots são não-objetivo, e a API de MODs não tem áudio (`mods.rs:88-112`). Música e soundboard também brigariam com «a voz nunca cede à tela».

### Perguntas ao dono

- A 1.0 promete que qualquer 1.x conversa com qualquer 1.x? Se sim, a etapa 2 (envelope de extensão) é obrigatória antes do lançamento; se não, a 1.0 precisa dizer nas notas que cada versão menor pode obrigar o grupo a atualizar junto.
- O cancelamento de eco é condição da 1.0 ou pode ir para a 1.1? Qual critério de medida o faria entrar (sugestão: ≥20 dB de ERLE num MacBook com alto-falante, +10 ms no máximo de latência)?
- Aperte-para-falar global: você aceita que a tecla escolhida fique «tomada» dos outros apps (atalho registrado, sem permissão extra) ou prefere o hook de baixo nível, que não toma a tecla mas pede permissão de Acessibilidade no macOS e não funciona no Wayland?
- Agora que existe supressão de ruído, a ativação por voz deve virar o padrão, com o aperte-para-falar como opção? Sem AEC, isso aumenta o eco para quem não usa fone.
- Linux é plataforma da 1.0? A v0.15.0 e a v0.14.2 saíram sem nenhum pacote Linux, e o Mac só em aarch64.
- Mobile sai formalmente da 1.0 (specs 00 e 09 ainda o preveem)?
- Existe disposição de pagar banda de um relé operado pelo projeto? Se não, o degrau 5 fica fora e a 1.0 só diagnostica o furo que não abre.
- Qual licença? O repositório DATA-AND-DEV/SEELE está público e o README diz «ainda não definida».
- Notificação do sistema traz uma dependência Tauri nova (tauri-plugin-notification): aceita, com adendo ao ADR 0020?
