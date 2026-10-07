# SEELE no iPhone sem pagar os US$ 99: rotas, limites e recomendação

**Data:** 2026-10-07. **Base:** sete frentes de pesquisa, cada uma com verificação adversarial. Afirmações refutadas foram retiradas, e as incertas aparecem marcadas como **[incerto]**.

**Conferido hoje no repositório e no Mac:**
- A release mais recente é a v0.15.0 (commit `e2fac4dab`, 2026-09-23) e não traz `.ipa`. Os assets são .dmg, .app.tar.gz, .exe, CLI, SHA256SUMS e latest.json [K8].
- O iPhone de teste aparece como `unavailable` no `devicectl` [K9].
- A branch `mobile/ios` tem 0 commits à frente da `main`. Todo o trabalho de iOS existe só na árvore de trabalho [K7].

As fontes ficam em listas numeradas ao fim de cada seção: A (Personal Team), B (sideload), C (navegador), D (rotas pagas, isenção e lei), X (o que não funciona) e K (código e medições locais). Fontes sem data aparecem com a data em que foram lidas.

---

## 1. A resposta curta

1. **Só você: sim, de graça.** O app Tauri que você já instala pelo Personal Team é permitido pela licença do Xcode ("personal, non-commercial use") e não usa nenhuma capability paga [A1][A3][A5]. O custo é reinstalar a cada 7 dias. O perfil atual vence em **12/10/2026 às 13:06 de Brasília** [A2].
2. **Amigos: dá tecnicamente, mas sai caro em atrito.** Os caminhos são o seu Personal Team em até 2 aparelhos extras, ou o AltStore Classic/SideStore com o Apple ID grátis de cada amigo [A1][B1]. Os problemas:
   - renovação semanal e no máximo 3 apps por aparelho;
   - onda de "provisioning profile is banned" desde julho/2026 [B11];
   - contraria a §2.4 da licença, que exige o programa pago quando um terceiro usa o app [A3].
3. **Público: sem pagar, só pelo navegador.** No Safari do iOS 26.4 ou mais novo, com WebTransport, sem Apple e sem instalar nada [C1][C2]. Mas nessa rota o iPhone só entra em servidores, nunca hospeda [C15]. Ela também exige trabalho no seeled, e nada foi medido num iPhone ainda [C20].
4. **Hospedar pelo iPhone tem limite de plataforma em qualquer rota, paga ou não:**
   - o servidor só vive enquanto houver sessão de áudio ativa [A6][A16];
   - a abertura automática da porta IPv4 por UPnP deve falhar, porque depende de multicast [A8][K1].
5. **Pagar não muda o que o app consegue fazer, só compra distribuição estável** para amigos e público. O preço é R$ 549,90/ano no app Apple Developer [D9], ou zero com isenção, que só vale para pessoa jurídica sem fins lucrativos [D10]. O canal no Brasil seria a AltStore PAL, onde os apps não expiram [D5][D6].

---

## 2. Comparação das rotas

| Rota | Exige programa pago? | Quem instala | Renova a cada | Hospeda? | Limites principais | Esforço no código do SEELE | Risco |
|---|---|---|---|---|---|---|---|
| **A. Personal Team pelo Xcode** (o que existe hoje) | Não [A1][A3] | Você, com Mac Apple silicon e Xcode 27 [A10] | ≤ 7 dias [A1][A2] | Sim, com ressalvas: só com áudio ativo, sem UPnP [A8][A16] | 3 aparelhos, 3 apps por aparelho, 10 App IDs. Sem push, associated domains nem multicast [A1][A5] | **P**: porteiras de iOS e renovação automática [K3][K5] | Baixo na licença. Alto no operacional: vence sem avisar e a reinstalação derruba o servidor |
| **A′. Seu Personal Team no iPhone de amigos** | Não, mas contraria a §2.4 [A3] | Até 2 amigos que vêm ao **seu** Mac [A1] | 7 dias, com o aparelho dele ao seu alcance | Igual a A | Developer Mode e pareamento com o seu Mac [A9] | P | Contratual (a Apple pode revogar perfis, §2.3) e logístico |
| **B. AltStore Classic / SideStore** (Apple ID grátis de cada um) | Não, mas contraria a §2.4 para terceiros [A3] | Amigos ou qualquer pessoa disposta [B1][B8] | 7 dias, renovação no aparelho (Wi-Fi + LocalDevVPN) [B4][B8] | Igual a A | 3 apps contando a própria ferramenta [B19]; AltStore 2.3 pede iOS 17.4+ [B1] | **M**: `.ipa` publicado, fonte JSON e versão real [B14][K11] | **Alto**: bans [B11], login quebrado em set/2026 [B10][B20], bundle ID reescrito (vira pessoa nova) [B13] |
| **B′. LiveContainer** (dentro de B) | Não | Igual a B | 7 dias | Plausível, não medido [B16] | Não aplica os entitlements do app de dentro e não separa os apps em sandbox [B16] | M | Alto, igual a B |
| **C. Cliente pelo navegador** (Safari ou web app na Tela de Início) | Não [C9] | Qualquer pessoa com iOS ≥ 26.4 [C2] | Nunca | **Não** [C15] | Sem compartilhar tela [C24]; voz com tela bloqueada incerta [C11][C12]; sem degrau 4 [K1] | **M** no seeled + **G a GG** no cliente [C20] | Médio: compatibilidade do wtransport com o Safari [C17]; nada medido no iPhone |
| **D. AltStore PAL no Brasil** (loja alternativa) | **Sim**, R$ 549,90/ano [D5][D9] | Público com iPhone, iOS 26.5+, conta brasileira e presente no Brasil (também UE e Japão) [B6] | Não expira (segundo a AltStore) [D6] | Igual a A, sujeito à regra 2.5.4 na notarização [D7] | Notarização a cada versão; MODs esbarram na 2.5.2 [D2][D7] | M [K11] | Recusa ou revogação da notarização [D17] |
| **E. Isenção da taxa** (associação, escola ou governo) | Programa sim, taxa não [D10] | Igual a D ou App Store | Igual a D | Igual a D | Só pessoa jurídica sem fins lucrativos, sem vender nada digital; elegibilidade do Brasil **[incerto]** [D10][D11] | Igual a D | Burocracia e conflito com a monetização [D22] |

---

## 3. As rotas viáveis, passo a passo

### 3.1 Rota A: Personal Team (só você, de graça)

**O que vale hoje**
- **Limites da conta grátis:** até 10 App IDs e 3 aparelhos, ambos expirando em 7 dias; até 3 apps por aparelho; perfil válido por 7 dias a partir da emissão, com "rebuild and reinstall" depois [A1]. Um engenheiro de DTS da Apple confirma os 7 dias [A21].
- **Medição local** [A2]:
  - o perfil do SEELE foi criado em 2026-10-05 16:06:56 UTC e vence em 2026-10-12 16:06:56 UTC (13:06 BRT), com TimeToLive=7;
  - o certificado Apple Development vale até 2027-10-05, então quem vence toda semana é o perfil, não o certificado;
  - o binário assinado leva só `application-identifier`, `team-identifier` e `get-task-allow`.
- **Licença:** a EA2002, datada de "06/08/2026" no formato americano, ou seja 8 de junho de 2026. A §2.2.B permite o "Your own personal, non-commercial use" em aparelhos seus [A3]. Uma resposta da DTS de abril de 2026 confirma que o Apple ID gratuito basta para o próprio aparelho [A4].
- **Capabilities:** a coluna gratuita da tabela da Apple inclui Background modes, e por isso `UIBackgroundModes=audio` funciona [A5][A6]. A permissão de rede local é só uma chave do Info.plist [A7]. Ficam de fora push, associated domains (universal links), iCloud, Network Extensions e Push to Talk [A5]. Multicast exige um entitlement que a Apple concede a pedido [A8]. Que o Personal Team não consiga esse entitlement é inferência forte, não está escrito em lugar nenhum **[incerto]**.
- **Xcode 27:** pareia sem cabo pelo Device Hub com iOS 27 ("Pair Nearby Device…"). Depois de atualizar o iOS é preciso parear de novo. Só roda em Mac Apple silicon com macOS Tahoe 26.6 ou mais novo [A10].
- **Linha de comando:** `xcodebuild -allowProvisioningUpdates` exige a conta logada no Xcode, em Settings > Apple Accounts [A11]. A saída JSON do `devicectl` é a interface estável; a saída em texto, não [A11]. O tauri-cli 2.11.4 passa `-allowProvisioningUpdates`, mas **não** passa `-allowProvisioningDeviceRegistration`, então o script não registra um aparelho novo sozinho [K6].

**Passo a passo para você**
1. **Antes de tudo, faça commit do trabalho de iOS.** A branch tem 0 commits à frente da main, e um `tauri ios init` novo reescreve `project.yml` e `main.mm` [K7].
2. **Antes de 12/10 às 13:06**, deixe o iPhone alcançável pelo Mac: mesma Wi-Fi, desbloqueado, ou com cabo. Hoje ele aparece `unavailable`, e nesse estado o `tools/ios-aparelho.sh` para com "nenhum iPhone conectado" [K9][K5].
3. Rode `APPLE_DEVELOPMENT_TEAM=<seu Team ID> tools/ios-aparelho.sh`. Ele recompila em debug, instala e abre o app [K5].
4. Para não depender de memória, automatize com um LaunchAgent no Mac rodando a cada cerca de 6 dias (esforço P). O script precisa de quatro ajustes:
   - ler `devicectl --json-output` em vez do texto;
   - não morrer no `process launch` quando o iPhone estiver bloqueado (hoje o `set -eu` derruba o script);
   - aceitar build de release;
   - rodar fora do horário de uso, porque a reinstalação mata o processo e o servidor hospedado [K5][A11].
5. Não se sabe ainda se o `devicectl` instala com o iPhone bloqueado e pareado só por Wi-Fi, nem por quanto tempo a sessão da conta no Xcode dura antes de pedir 2FA de novo **[incerto]**.

**Passo a passo para um amigo (duas variantes, ambas com atrito alto)**
- **Pelo seu Mac** (cabe em 2 aparelhos além do seu) [A1]:
  1. o amigo pareia o iPhone com o seu Mac, por cabo ou sem fio se tiver iOS 27 [A10];
  2. liga o Developer Mode em Ajustes > Privacidade e Segurança, o que pede reinício e o código do aparelho. A chave só aparece depois de iniciar o pareamento [A9];
  3. você instala;
  4. ele confia no desenvolvedor em Ajustes > Geral > VPN e Gerenciamento de Dispositivo. Isso vem de relatos de usuários em fórum, não de documentação [A19]. Para apps enterprise, a Apple exige internet nessa verificação [A20], então pode ser preciso internet aqui também **[incerto]**;
  5. ele volta ao alcance do seu Mac toda semana.

  Isso contraria a §2.4 [A3]. Como o tauri-cli não registra aparelhos novos, o primeiro registro precisa ser feito pelo Xcode ou por `xcodebuild -allowProvisioningDeviceRegistration` [K6].
- **O amigo compila sozinho:**
  - precisa de Mac Apple silicon com macOS Tahoe 26.6+, Xcode 27, Rust e Tauri [A10], mais o próprio Apple ID e **outro bundle ID**. O `tech.datadev.seele` fica preso ao seu Personal Team enquanto não expira, e a DTS recomenda trocar o ID [A12];
  - hoje ele teria de editar `tauri.conf.json:5` e o script, que fixam esse ID [K5];
  - a licença fica ambígua, porque "Application" é definida como software "developed by You" [A3];
  - o repositório não tem LICENSE, então ele precisa da sua permissão por escrito [D22].

**Hospedar nesta rota: o que funciona e o que não**
- **Funciona:** QUIC/UDP unicast na LAN, com a permissão de rede local [A7], e áudio em segundo plano [A6].
- **Fica de pé só com áudio ativo.** À pergunta de como montar um servidor de rede em segundo plano, a DTS responde "You can't" [A16]. Uma nota antiga da Apple (2011) avisa que, com o app suspenso, o sistema pode recuperar o socket que escuta, e ele não volta mesmo quando o app volta [A17]. O código não trata ciclo de vida nem interrupção da sessão de áudio [K1]. Uma ligação telefônica ou um intervalo sem áudio pode derrubar o servidor sem aviso.
- **UPnP deve falhar.** O degrau 3 descobre o roteador por SSDP em 239.255.255.250:1900, que é multicast [K1][A8]. O PCP do SEELE só abre o firewall IPv6, e o próprio `porta.rs` diz que "NAT-PMP e PCP ficaram de fora" do IPv4. **No iPhone não sobra nenhum caminho automático para a porta IPv4** [K1].
- **O produto sabe e não conta, caso 1:** quando isso falha, a mensagem `NenhumRoteador` manda procurar "UPnP" nos ajustes do roteador [K1], o que não resolve nada no iPhone.
- **Contorno a medir:** o igd-next permite trocar `broadcast_address` por `gateway:1900`, fazendo uma busca M-SEARCH unicast que dispensa o entitlement. Não se sabe se os roteadores respondem a isso [K1].
- **Permissão de rede local:** mandar UDP unicast a um endereço da LAN exige a permissão. Com o app em segundo plano e a permissão ainda indefinida, o iOS nega sem perguntar. Por isso o pedido de permissão tem de acontecer em primeiro plano, antes de hospedar [A7].
- **O produto sabe e não conta, caso 2:**
  - o botão ATUALIZAR aparece e falha com `UnsupportedOs` ("expected one of linux, darwin or windows");
  - o seletor de versões do ADR 0046 depende de `spawn`, e apps de iOS não podem criar processos filhos [K3][A18];
  - o perfil vence em silêncio, embora o app tenha o `ExpirationDate` no `embedded.mobileprovision` e pudesse avisar antes [A2].
- **Dados na reinstalação [incerto].** Identidade, pinos e bancos dos servidores hospedados ficam no contêiner do app [K4]. Segundo a DTS, reinstalar por cima com o mesmo bundle ID "should" manter os dados [A14]. Ninguém mediu.
- **Internet na primeira abertura [incerto].** O check-in com ppq.apple.com está documentado só para times pagos criados depois de 2021-06-06. Para a conta grátis, há só um relato de usuário [A15].

**Fontes da rota A**
- [A1] https://developer.apple.com/help/account/basics/about-your-developer-account (a antiga https://developer.apple.com/support/compare-memberships/ hoje redireciona para ela). Sem data, lida em 2026-10-07.
- [A2] Medição local em 2026-10-07: o perfil instalado pelo Xcode e `apps/seele-app/gen/apple/build/arm64/SEELE.ipa` (security cms -D, openssl x509, codesign -d).
- [A3] https://www.apple.com/legal/sla/docs/xcode.pdf. EA2002 de 8/6/2026 (rodapé "06/08/2026"; Last-Modified 2026-08-05). §2.2.B, §2.3, §2.4, §5.
- [A4] https://developer.apple.com/forums/thread/823137. DTS, abril de 2026.
- [A5] https://developer.apple.com/help/account/reference/supported-capabilities-ios. Lida em 2026-10-07. O Last-Modified de 2026-10-05 é a data de publicação do site, não de revisão do conteúdo.
- [A6] https://developer.apple.com/documentation/xcode/configuring-background-execution-modes. Lida em 2026-10-07.
- [A7] https://developer.apple.com/documentation/technotes/tn3179-understanding-local-network-privacy. Revisão mais recente em 2026-10-06.
- [A8] https://developer.apple.com/documentation/bundleresources/entitlements/com.apple.developer.networking.multicast. Lida em 2026-10-07.
- [A9] https://developer.apple.com/documentation/xcode/enabling-developer-mode-on-a-device. Lida em 2026-10-07.
- [A10] https://developer.apple.com/documentation/xcode/managing-your-simulated-and-physical-devices-in-device-hub e https://developer.apple.com/documentation/xcode-release-notes/xcode-27-release-notes (notas 179418483 e 162138432). Xcode 27.0, lidas em 2026-10-07.
- [A11] `man xcodebuild` e `xcrun devicectl help` (Xcode 27.0 27A266a, devicectl 642.16). Lidos localmente em 2026-10-07.
- [A12] https://developer.apple.com/forums/thread/843925. Quinn (DTS), agosto de 2026.
- [A13] https://help.apple.com/xcode/mac/current/en.lproj/dev17411c009.html. Sem data, lida em 2026-10-07.
- [A14] https://developer.apple.com/forums/thread/807785. DTS, novembro de 2025.
- [A15] https://developer.apple.com/help/account/provisioning-profiles/provisioning-profile-updates (lida em 2026-10-07) e https://developer.apple.com/forums/thread/750779 (abril de 2024).
- [A16] https://developer.apple.com/forums/thread/685525. Quinn (DTS), 2021-07-22, revisado em 2026-01-09.
- [A17] https://developer.apple.com/library/archive/technotes/tn2277/_index.html. 2011, não é mais atualizada.
- [A18] https://developer.apple.com/forums/thread/747499. Resposta da DTS; a pesquisa não registrou a data.
- [A19] https://developer.apple.com/forums/thread/685271. 2021, relatos de usuários.
- [A20] https://support.apple.com/en-us/HT204460. Publicado em 2026-06-04.
- [A21] https://developer.apple.com/forums/thread/796061. DTS, agosto de 2025.

---

### 3.2 Rota B: sideload com o Apple ID grátis de cada amigo (AltStore Classic / SideStore)

**O que vale hoje**
- **AltStore Classic 2.3:**
  - traz o "Remote AltServer", que instala e renova sem computador, por Wi-Fi, e pede iOS 17.4+. As notas de versão são de 14/09/2026 [B1]; a tag v2.3 no GitHub aparece com 30/09/2026, discrepância de datas não resolvida;
  - no iOS 27, o Remote AltServer se configura inteiro no aparelho. Para quem está no Brasil, na UE ou no Japão, o AltStore Classic pode ser instalado pela AltStore PAL, sem tocar num computador [B2][B3];
  - exige Wi-Fi (rede celular não serve) e o LocalDevVPN conectado, e o servidor é escolhido numa lista. A página não diz quem opera esses servidores nem o que passa por eles [B4].
- **LocalDevVPN:** gratuito na App Store brasileira, versão 1.3.0 de 2026-08-31 [B5].
- **AltStore PAL no Brasil:** exige iOS 26.5+, Safari, conta brasileira **e estar fisicamente no Brasil**, e vale só para iPhone [B6][B7]. Que o PAL seja gratuito para quem instala em 2026 não está confirmado em fonte desta data **[incerto]**.
- **SideStore:**
  - pede iOS 15+, um computador só na instalação inicial (com o iloader), Wi-Fi e o LocalDevVPN [B8];
  - o arquivo de pareamento expira ao atualizar ou resetar o iPhone, e também "at random times" [B9];
  - a versão 0.6.4 está marcada "DO NOT USE", porque mudanças no servidor da Apple quebraram o login. A correção existe só no 0.7.0-alpha (2026-09-15) [B10];
  - a conta conta o próprio SideStore entre os 3 apps, então sobram 2 vagas [B19].
- **Banimento.** O erro 0xe8008024 "provisioning profile is banned" aparece desde pelo menos 2026-07-10, no SideStore, no iloader, no Sideloadly e no AltStore [B11]. Há relatos de apps já instalados que ficaram "unverified" antes dos 7 dias. A causa não foi confirmada, e o único contorno relatado é trocar de conta, o que apaga os dados [B11]. O AltStore recomendou um Apple ID temporário em 21/09/2026 [B12]. Isso esbarra na §2.3 da licença, que proíbe ter mais Apple Accounts do que o necessário [A3].
- **O AltStore reescreve o bundle ID** para `<bundle>.<TEAMID>` [B13]. Passar a instalação do Xcode para o AltStore, ou trocar de Apple ID, gera contêiner novo, logo pessoa nova e servidor com impressão (fp) nova [K4]. Quem estiver conectado vê "A CHAVE DO SERVIDOR MUDOU", o alerta reservado para ataque (ADR 0003). Para o SideStore, que é um fork, isso é inferência **[incerto]**.
- **Fonte de apps:** o projeto pode publicar uma "source" (um JSON mais o `.ipa`). Os apps declaram permissões de privacidade, e o `buildVersion` tem de bater com o `CFBundleVersion` [B14]. Hoje o `CFBundleVersion` é 0.0.0, e o Info.plist também tem `NSScreenCaptureUsageDescription` [K11]. O SideStore lê o mesmo formato, mas recusa fontes com os campos `marketplaceID` e "Build" gerados por ferramenta [B15].
- **LiveContainer:**
  - contorna o limite de 3 apps e 10 App IDs;
  - não aplica os entitlements do app de dentro e não separa os apps em sandbox: um lê os dados do outro;
  - o Info.plist dele declara microfone, rede local, `audio` e `voip` em segundo plano e o `UIApplicationSceneManifest` (a frente que disse o contrário foi refutada);
  - a 3.8.0 (2026-07-17) suporta o beta do iOS 27; para o iOS 27 final só há nightly (2026-09-18) [B16].
  
  Ninguém testou o SEELE dentro dele.
- **Sideloadly e Impactor:** renovam só com um computador ligado na mesma rede ou com o iPhone no USB [B18].

**Passo a passo para você (quem publica)**
1. Gerar um `.ipa` no CI. O `release.yml` não tem job de iOS [K11], e não está medido se `cargo tauri ios build --no-sign` entrega um `.ipa` [K6] **[incerto]**.
2. Pôr a versão real no `gen/apple/project.yml`, que hoje fixa 0.0.0 [K11].
3. Publicar uma fonte JSON no SEELE-RELEASES, declarando microfone, rede local e captura de tela [B14].
4. Saber que publicar o `.ipa` para terceiros fica fora do contrato (§2.4) [A3].

**Passo a passo para um amigo**
- **Cenário A: no Brasil, com iOS 27, sem computador** [B2][B3][B4][B6]
  1. No Safari, instalar a AltStore PAL pelo altstore.io e autorizar o marketplace em Ajustes.
  2. Na PAL, instalar o AltStore Classic.
  3. Instalar o LocalDevVPN pela App Store [B5].
  4. No AltStore Classic, abrir "Set up Remote AltServer…" e seguir o guia.
  5. Entrar com um Apple ID.
  6. Ligar o Developer Mode e reiniciar. Não se sabe se a chave aparece sem nunca ter pareado com um Mac **[incerto]** [A9].
  7. Abrir o link `altstore://source?url=…` e instalar o SEELE.
  8. Confiar no perfil em Ajustes > Geral > VPN e Gerenciamento de Dispositivo.
  9. Manter Wi-Fi e LocalDevVPN ligados para a renovação de 7 dias acontecer.
- **Cenário B: iOS 26, ou fora do Brasil** [B8][B9][B17]
  1. Uma vez, num PC ou Mac, usar o iloader para instalar o SideStore. O arquivo de pareamento vai junto.
  2. Confiar no perfil, ligar o Developer Mode e conectar o LocalDevVPN.
  3. Adicionar a fonte com `sidestore://source?url=…`.
  4. Renovar no aparelho.
  5. Refazer o pareamento no computador depois de cada atualização do iOS, ou quando ele expirar sozinho.

**Hospedar nesta rota:** vale tudo o que vale na rota A, já que o binário é o mesmo, re-assinado. A diferença é a falha da renovação: um ban ou um pareamento expirado derruba o servidor sem aviso [B11][B9]. O PR #846 do SideStore (mesclado em 2025-01-19) indica que a renovação só troca o perfil sem reinstalar, então um servidor em execução talvez não caia na renovação. Isso não foi medido **[incerto]**.

**Fontes da rota B**
- [B1] https://faq.altstore.io/release-notes/altstore. Versão 2.3 de 2026-09-14. Tag no GitHub: https://github.com/altstoreio/AltStore/releases/tag/v2.3 (2026-09-30).
- [B2] https://fosstodon.org/@altstore/117271149448802444. 2026-09-14.
- [B3] https://www.threads.com/@rileytestut/video/DdR99O2jwKh/. 2026-09-14.
- [B4] https://faq.altstore.io/altstore-classic/remote-altservers. Sem data, lida em 2026-10-07.
- [B5] https://itunes.apple.com/lookup?id=6755608044&country=br. Versão de 2026-08-31, consultada em 2026-10-07.
- [B6] https://support.apple.com/en-us/118110. 2026-06-18.
- [B7] https://9to5mac.com/2026/06/18/altstore-pal-now-available-in-brazil-as-apple-flips-the-switch-on-alternative-marketplaces/. 2026-06-18.
- [B8] https://docs.sidestore.io/docs/installation/prerequisites e https://docs.sidestore.io/docs/installation/install. Sem data, lidas em 2026-10-07.
- [B9] https://docs.sidestore.io/docs/advanced/pairing-file. Sem data, lida em 2026-10-07.
- [B10] https://github.com/SideStore/SideStore/releases. 0.6.3 (2026-05-05), 0.6.4 (2026-09-09), 0.7.0-alpha (2026-09-15).
- [B11] https://github.com/SideStore/SideStore/issues/1365 (desde 2026-07-10); https://github.com/nab138/iloader/issues/583 (2026-07-29 a 2026-09-18); https://github.com/SideloadlyiOS/Sideloadly-Download/issues/18 (2026-09-20).
- [B12] https://fosstodon.org/@altstore/117310342617792479. 2026-09-21.
- [B13] https://github.com/altstoreio/AltStore/blob/master/AltStore/Operations/FetchProvisioningProfilesOperation.swift (linhas 171-191). Lido em 2026-10-07.
- [B14] https://faq.altstore.io/developers/make-a-source e https://faq.altstore.io/developers/distribute-with-altstore-classic. Sem data, lidas em 2026-10-07.
- [B15] https://docs.sidestore.io/docs/advanced/app-sources. Sem data, lida em 2026-10-07.
- [B16] https://github.com/LiveContainer/LiveContainer (README, releases e `LiveContainer/Info.plist` da main). Lidos em 2026-10-07.
- [B17] https://github.com/nab138/iloader/releases. v2.3.6 (2026-10-06), v2.3.2 (2026-09-10).
- [B18] https://sideloadly.io/faq (lida em 2026-10-07); https://github.com/claration/impactor (v2.6.5, 2026-09-28).
- [B19] https://faq.altstore.io/altstore-classic/activating-apps (lida em 2026-10-07), mais o FAQ do SideStore: "3 apps (including itself)".
- [B20] https://fosstodon.org/@altstore/117219567926635253. 2026-09-05 a 2026-09-08.

---

### 3.3 Rota C: cliente pelo navegador (a única grátis para o público)

**O que vale hoje**
- **WebTransport:** chegou ao Safari 26.4 com streams e datagramas. Quando não há QUIC, cai para HTTP/2 sobre TCP [C1]. O caniuse marca suporte no iOS 26.4+, incluindo 27.0 e 27.1–27.2 [C2].
- **`serverCertificateHashes`:**
  - o MDN marca suporte no Safari 26.4, mas no iOS os dados são "mirror", copiados do macOS e não medidos no iPhone [C3];
  - o WebKit implementou no bug 300057 (2025-10-02) [C4];
  - a especificação exige certificado X.509v3 com validade total ≤ 2 semanas, aceita ECDSA P-256, recusa RSA, usa SHA-256 do DER e **aceita vários hashes ao mesmo tempo** [C5];
  - o código do WebKit compara a string `"sha-256"` literalmente (o spike já usa minúsculas) e, com `requireUnreliable:true`, desliga o recuo para TCP [C6].
- **Áudio:** AudioEncoder e AudioDecoder chegaram no Safari 26.0 [C7]. O Opus do WebCodecs vai até 2 canais e não aceita Ogg [C3]. O `getUserMedia` funciona em web app da Tela de Início desde o iOS 13.4 [C8].
- **Web Push:** funciona só em web app da Tela de Início e **dispensa o Apple Developer Program** [C9]. Desde o iOS 26, todo site adicionado à Tela de Início abre como web app por padrão [C21]. O Wake Lock vale em web apps desde o 18.4 [C10].
- **Microfone com a tela bloqueada [incerto]:**
  - a explicação de que o web app perde o microfone porque não declara `audio` foi **refutada**. No runtime do iOS 27.0, o Web.app que hospeda os web apps declara `audio` em `UIBackgroundModes` [C11];
  - os indícios de corte são relatos de 2023 [C13] e um teste no simulador do iOS 26.2, em que a track fica muda e volta quando a página fica visível [C12];
  - o Safari 27.0 corrigiu a AudioSession para continuar ativa enquanto o microfone captura [C14];
  - outro bug, ainda aberto, fala de track que termina após 30 s em segundo plano [C23].
  
  Só medindo.
- **Não hospeda:** o WebKit fechou como WONTFIX a API de sockets TCP/UDP, por ser "too low-level" [C15].
- **Bug aberto de controle de fluxo:** a conexão trava em 16 MB ou 7600 streams [C16]. Foi reproduzido no Firefox do iOS 26.6.1, que usa WKWebView [C16].
- **O risco técnico mais imediato:** o wtransport 0.7.2, a versão do spike e a mais nova, só anuncia códigos de SETTINGS de rascunhos antigos. O draft-ietf-webtrans-http3-16 (julho de 2026) usa outros códigos e exige `reset_stream_at` [C17]. A primeira tentativa no iPhone pode falhar antes de chegar à voz.
- **No seeled, o certificado não serve como está:** ele vale de 1975 a 4096, e a impressão (fp) é o SHA-256 do DER, o mesmo valor do `serverCertificateHashes` [K1][C20]. Dá para resolver de dois jeitos: um segundo certificado curto escolhido pelo ALPN `h3`, ou um certificado curto com a mesma chave e o pino migrando para o SPKI, o que mexe na regra do ADR 0003. A troca a cada 14 dias **não força convite novo**: basta pré-gerar certificados com janelas futuras e mandar vários hashes [C5][C6]. Não se mediu como o Safari lida com listas longas **[incerto]**.
- **Alcance:** o navegador não usa o degrau 4 (furo de NAT pelo ponto de encontro). Amigos atrás de CGNAT, sem IPv6 e sem porta aberta no servidor, não conectam [K1].
- **Rede local:** o tráfego do Safari e do WKWebView dispensa o aviso de Rede Local [A7].
- **Adoção:** 79% dos iPhones usavam iOS 26 em 2026-06-07. A Apple não separa quantos estão no 26.4 ou acima [C18].
- **Spike:** mediu só em localhost, no Chromium. A tabela de medidas no celular está vazia [C20].

**Passo a passo para você**
1. Medir o spike no iPhone, como descrito na seção 5.
2. Se passar, escrever um ADR, porque a rota colide com os ADRs 0003, 0022 e 0047 e com as specs 06 e 09 [K10][C20].
3. Implementar o endpoint `h3` no mesmo socket quinn (esforço M) e o cliente (esforço G para voz e texto, GG para paridade) [C20].
4. Servir a página estática por HTTPS gratuito, por exemplo em github.io [C19].

**Passo a passo para um amigo:** abrir o link de convite no Safari, com iOS 26.4 ou mais novo. Para ter ícone e notificações, tocar em Compartilhar e depois em Adicionar à Tela de Início [C21][C9]. Não precisa de Mac, de Apple ID de desenvolvedor nem de renovação.

**Fontes da rota C**
- [C1] https://webkit.org/blog/17862/webkit-features-for-safari-26-4/. 2026-03-24.
- [C2] https://caniuse.com/webtransport. Consultado em 2026-10-07.
- [C3] https://github.com/mdn/browser-compat-data (api/WebTransport.json, WebTransportDatagramDuplexStream.json, AudioEncoder.json). Consultado em 2026-10-07.
- [C4] https://bugs.webkit.org/show_bug.cgi?id=300057. 2025-10-02.
- [C5] https://www.w3.org/TR/webtransport/. CR Snapshot de 2026-07-30.
- [C6] https://github.com/WebKit/WebKit/blob/main/Source/WebKit/NetworkProcess/webtransport/cocoa/NetworkTransportSessionCocoa.mm. Consultado em 2026-10-07.
- [C7] https://webkit.org/blog/17333/webkit-features-in-safari-26-0/. 2025-09-15.
- [C8] https://bugs.webkit.org/show_bug.cgi?id=185448. 2020-02-06.
- [C9] https://webkit.org/blog/13878/web-push-for-web-apps-on-ios-and-ipados/. 2023-02-16.
- [C10] https://webkit.org/blog/16574/webkit-features-in-safari-18-4/. 2025-03-31.
- [C11] Medição em 2026-10-07: Info.plist de `Web.app` e `MobileSafari.app` no runtime do Simulador iOS 27.0 (24A434), Xcode 27. É o Info.plist, não o comportamento.
- [C12] https://github.com/MiguelMedeiros/ghostly/pull/1377. 2026-10-07.
- [C13] https://bugs.webkit.org/show_bug.cgi?id=239602. Último comentário em 2023-12-26.
- [C14] https://webkit.org/blog/18325/webkit-features-for-safari-27-0/. 2026-09-17.
- [C15] https://bugs.webkit.org/show_bug.cgi?id=144537. WONTFIX em 2023-03-27.
- [C16] https://bugs.webkit.org/show_bug.cgi?id=319818. Aberto em 2026-07-20; comentário de 2026-08-26.
- [C17] wtransport 0.7.2 (crates.io, 2026-08-11; `settings.rs` lido em 2026-10-07) e https://www.ietf.org/archive/id/draft-ietf-webtrans-http3-16.txt (julho de 2026).
- [C18] https://developer.apple.com/support/app-store/. 2026-06-07.
- [C19] https://docs.github.com/en/pages/getting-started-with-github-pages/securing-your-github-pages-site-with-https. Consultado em 2026-10-07.
- [C20] `spikes/voz-no-navegador/README.md` (linhas 80-110), `src/main.rs` e `docs/monetizacao-2026-10-04.md` §3.7. Todos de 2026-10-04.
- [C21] https://webkit.org/blog/16993/news-from-wwdc25-web-technology-coming-this-fall-in-safari-26-beta/. 2025-06-09.
- [C23] https://bugs.webkit.org/show_bug.cgi?id=204681. 2020-03-23, ainda aberto.
- [C24] MDN, `api/MediaDevices.json`: `getDisplayMedia` não existe no Safari do iOS. Consultado em 2026-10-07.

---

### 3.4 Rotas D e E: o que pagar compra (e o caminho de taxa zero)

**Nenhuma rota legal dispensa o programa pago.**
- **Brasil:** o CADE homologou o acordo (TCC) com a Apple em 23/12/2025 [D4]. Desde o iOS 26.5, com o anúncio de 18/06/2026, existem lojas alternativas [D1]. Todo app distribuído nelas precisa ser notarizado pelo App Store Connect, e o App Store Connect só existe para membro do programa [D2][A1]. No Brasil **não existe instalação direta pelo site do desenvolvedor**; isso só existe na UE [B6].
- **UE:** a distribuição pelo site exige critérios financeiros [D20].
- **Japão:** também exige notarização [D2].
- **EUA:** não há distribuição alternativa. O caso Epic trata só de links de pagamento [D21].
- **Operar uma loja própria** exige ser organização e ter carta de crédito de US$ 1 milhão, ou dois anos de programa mais 1 milhão de instalações anuais [D18].

**AltStore PAL, pagando**
- **Fluxo:** notarizar, baixar o pacote de distribuição alternativa, hospedá-lo sem alterar nada e publicar uma "source". A fonte pode ficar fora da vitrine, só para quem tiver o link [D5].
- **Expiração:** segundo o FAQ da AltStore, os apps do PAL não expiram [D6].
- **Comissão:** quem não vende bens digitais não paga nada à Apple. Quem vende fora da App Store paga a CTC de 5% [D3].
- **Revisão na notarização:**
  - aplicam-se as regras 2.5.2 (baixar código executável, o que pega os MODs), 2.5.4 (modos de segundo plano só para o fim declarado), 2.4.2 e 4.7 [D7][D2];
  - **a regra 1.2 (filtro, denúncia, bloqueio) é [incerto]:** duas verificações leram o mesmo HTML das guidelines e divergem sobre se ela entra na notarização.
- **Revogação:** o iTorrent teve a notarização revogada em 2025, por sanções e não por malware. O Mini vMac teve a notarização recusada em 2024, não revogada [D17].
- **Preço:** R$ 549,90/ano dentro do app Apple Developer, lido em 2026-10-07 [D9]. Pelo site, a cobrança pode sair em USD, com câmbio e IOF [D9].
- **A mesma conta resolve o Developer ID do macOS**, que a monetização já considera [D22].

**O que pagar não compra**
- TestFlight serve só para beta de pré-lançamento (DPLA §7.4) [D8]. Também não pode ser recompensa de patrocínio (guideline 2.2), e cada build vale 90 dias [D7][D19].
- Ad hoc atende até 100 iPhones por ano, só para pessoas afiliadas à organização [D8].
- Nenhuma conta, paga ou não, permite servidor de rede em segundo plano [A16].

**E: taxa zero**
- **Isenção:** só para organização sem fins lucrativos reconhecida, instituição de ensino credenciada ou governo. Exclui pessoa física, empresário individual e empresa de uma pessoa só, e proíbe vender bens digitais em qualquer app [D10]. Segundo fonte secundária, a Apple trata o MEI como pessoa física [D13].
- **Brasil elegível hoje [incerto]:** só uma matéria de 2020 diz que sim [D11]. A página atual não lista países [D10].
- **Inscrição como organização:** exige D-U-N-S, site com domínio próprio e e-mail no domínio [D12].
- **Conflito interno:** a isenção conflita com a "chave de apoiador" cogitada na monetização [D22].
- **Variante:** entrar no time de uma universidade que já tem conta. A DTS sugeriu isso em outubro de 2025 [D14]. O Ad hoc, porém, só alcança pessoas afiliadas à instituição.

**Pagar sem tirar do bolso:** o GitHub Sponsors não cobra taxa de patrocínio vindo de conta pessoal [D15]. A Open Source Collective cobra 10% e exige licença aberta, e o repositório não tem LICENSE [D15][D22]. O Jellyfin paga a anuidade da Apple pelo Open Collective; a de 2025 foi paga em 2025-11-06 [D16].

**Fontes das rotas D e E**
- [D1] https://developer.apple.com/news/?id=dhwadr2x. 2026-06-18.
- [D2] https://developer.apple.com/support/app-distribution-in-brazil/. Lida em 2026-10-07.
- [D3] https://www.apple.com/newsroom/2026/06/apple-announces-changes-to-ios-in-brazil/. 2026-06-18.
- [D4] https://www.gov.br/cade/pt-br/assuntos/noticias/cade-forma-maioria-pela-homologacao-de-tcc-em-investigacao-sobre-praticas-da-apple-no-ios. 2025-12-23. Processo administrativo 08700.009531/2022-04; TCC 08700.006953/2025-62; texto integral ainda não publicado.
- [D5] https://faq.altstore.io/developers/distribute-with-altstore-pal. Sem data, lida em 2026-10-07.
- [D6] https://faq.altstore.io/altstore-pal/what-is-altstore-pal. Lida em 2026-10-07.
- [D7] https://developer.apple.com/app-store/review/guidelines/. Last Updated 2026-06-08.
- [D8] https://developer.apple.com/support/terms/apple-developer-program-license-agreement/. DPLA de 2026-08-18.
- [D9] https://apps.apple.com/br/app/apple-developer/id640199958 (lido em 2026-10-07) e https://developer.apple.com/support/enrollment/ (lida em 2026-10-07).
- [D10] https://developer.apple.com/support/membership-fee-waiver/ e https://developer.apple.com/help/account/membership/fee-waivers/. Lidas em 2026-10-07.
- [D11] https://macmagazine.com.br/post/2020/02/04/mais-paises-ganham-isencao-da-taxa-do-apple-developer-program/. 2020-02-04.
- [D12] https://developer.apple.com/help/account/membership/D-U-N-S/. Lida em 2026-10-07.
- [D13] https://www.tabnews.com.br/adevelop/inscricao-no-apple-developer-program-o-que-a-apple-realmente-exige-e-onde-as-pessoas-travam. Valores conferidos em 2026-09-13; fonte secundária.
- [D14] https://developer.apple.com/forums/thread/803362. Outubro de 2025.
- [D15] https://docs.github.com/en/sponsors/getting-started-with-github-sponsors/about-github-sponsors e https://docs.oscollective.org/welcome-and-introduction-to-osc/fees. Lidas em 2026-10-07.
- [D16] https://opencollective.com/jellyfin/expenses/272554. 2025-11-06.
- [D17] https://mjtsai.com/blog/2024/11/26/mini-vmac-for-ios-rejected-via-notarization/ (2024-11-26) e https://heise.de/-10625008 (2025).
- [D18] https://developer.apple.com/support/alternative-app-marketplace-br/. Lida em 2026-10-07.
- [D19] https://developer.apple.com/help/app-store-connect/test-a-beta-version/testflight-overview/. Lida em 2026-10-07.
- [D20] https://developer.apple.com/support/web-distribution-eu/. Lida em 2026-10-07.
- [D21] https://law.justia.com/cases/federal/appellate-courts/ca9/25-2935/25-2935-2025-12-11.html (2025-12-11); Suprema Corte, caso nº 25-1311 (petição de mérito da Apple de 2026-09-14).
- [D22] `docs/monetizacao-2026-10-04.md` (§3.3, §6 e linha 44, sem LICENSE). 2026-10-04.

---

## 4. O que não funciona, e por quê

- **Apps "contêiner" da App Store** não rodam o seeled:
  - a-Shell: o WebAssembly dele não tem sockets [X1];
  - iSH: emula i386 sem JIT, tem issues antigas e sem resposta de pânico com tokio, e só continua em segundo plano com um truque de localização [X2];
  - Scriptable: não tem API de socket [X3];
  - Pythonista: custa R$ 59,90 e não instala extensões em C, e o próprio aioquic tem extensões em C [X4];
  - UTM SE: sem JIT, "will run much slower" [X5].
- **TrollStore:** só funciona até o iOS 17.0 [X6]. **Feather:** usa certificados do programa pago [X7].
- **SideInstaller e "serviços de assinatura":** usam certificados corporativos de terceiros e pedem um perfil de DNS. É risco de segurança e viola o Enterprise Program [X8][X10]. Descartar.
- **JIT:** irrelevante, porque o SEELE é Rust compilado antes da execução [B16]. **Mac com Apple silicon:** roda apps de iPhone no Mac, mas não põe nada no iPhone [X11].
- **Swift Playground:** só existe para iPad e Mac, aceita só Swift (segundo fóruns) e o envio à loja pede conta paga [X9].
- **"App casca" de terceiros:** é o mesmo WebKit do Safari, sem o núcleo Rust. Não ganha nada sobre a rota C [C6][D7].
- **Hospedar pelo navegador:** impossível [C15]. **Servidor em segundo plano sem áudio:** impossível em qualquer conta [A16].
- **Distribuição por lei sem pagar:** não existe em nenhum país [D2][D20][D21]. **Distribuição pelo site do desenvolvedor no Brasil:** não existe [B6].
- **TestFlight como canal permanente ou recompensa de patrocínio:** viola o DPLA §7.4 e a guideline 2.2 [D8][D7].

**Fontes da seção 4**
- [X1] https://github.com/holzschu/a-shell e https://itunes.apple.com/lookup?id=1473805438&country=br (versão 2.2.2, 2026-09-21).
- [X2] https://github.com/ish-app/ish, issues #2447 (2024-08-27) e #805 (2020-06-28), e a wiki Running-in-background (2025-03-12).
- [X3] https://docs.scriptable.app/. Lida em 2026-10-07.
- [X4] https://itunes.apple.com/lookup?id=1085978097&country=br (Pythonista 3.4, 2023-04-27) e `setup.py` do aioquic na main (2026-10-07).
- [X5] https://docs.getutm.app/installation/ios/. UTM SE 4.7.5, 2026-01-11.
- [X6] https://github.com/opa334/TrollStore. README lido em 2026-10-07.
- [X7] https://github.com/claration/feather. v2.9.0, 2026-07-05.
- [X8] https://github.com/FrizzleM/SideInstaller (2026-10-05) e https://sideinstaller.net/.
- [X9] https://developer.apple.com/swift-playgrounds/ e https://itunes.apple.com/lookup?id=908519492&country=br (versão 4.7, 2026-03-17).
- [X10] https://developer.apple.com/programs/enterprise/. Lida em 2026-10-07.
- [X11] https://developer.apple.com/documentation/apple-silicon/running-your-ios-apps-in-macos. Lida em 2026-10-07.

---

## 5. Recomendação

**Agora, esta semana**
1. **Fazer commit do trabalho de iOS.** É o maior risco imediato: perder a árvore de trabalho é perder o app [K7].
2. **Renovar antes de 12/10 às 13:06.** O iPhone está `unavailable` no Mac [K9].
3. **Consertar o que o produto sabe e não conta.** Tudo isso é esforço P e vale para todas as rotas nativas [K1][K3][K5][A2]:
   - esconder ou recusar com nome o ATUALIZAR e o seletor de versões no iOS;
   - trocar a frase do `NenhumRoteador` no iPhone, que hoje manda ligar UPnP no roteador;
   - avisar antes de o perfil vencer, lendo o `ExpirationDate`;
   - registrar o esquema `seele://` com `CFBundleURLTypes`, que não pede capability [A5];
   - script de renovação lendo `--json-output`.
4. **Escrever um ADR decidindo se o celular hospeda.** A spec 06, o M6 e o README do spike dizem que não [K10]. Sem isso, a próxima revisão vai cobrar o celular por um requisito que as specs proíbem.

**O que medir primeiro, em ordem**
1. **O spike do navegador no iPhone.** Sim, ele vem primeiro, porque é a única rota grátis que chega a terceiros:
   - (a) o Safari do iOS 27 abre sessão com o wtransport 0.7.2? É aí que a rota pode morrer [C17];
   - (b) o hash do certificado de 13 dias é aceito, e um hash adulterado é recusado?
   - (c) `AudioEncoder.isConfigSupported` com Opus mono a 48 kHz responde que sim?
   - (d) a voz numa aba do Safari e num web app sobrevive 30 minutos com a tela bloqueada, como pede o aceite do M6?
   
   O ponto (d) pode começar já no Simulador do iOS 27 (repetindo a sonda do Ghostly) [C11][C12]. O resto precisa do aparelho.
2. **O app nativo hospedando no iPhone:**
   - o UPnP falha, e de que jeito (erro ou silêncio)?
   - o PCP abre o firewall IPv6?
   - o contorno M-SEARCH unicast para `gateway:1900` funciona?
   - o servidor aguenta 30 min com a tela bloqueada, uma ligação telefônica e um intervalo sem áudio? [K1][A16][A17]
3. **O vencimento:** deixar o perfil de um build de teste vencer de propósito, fora de uso. Medir o que aparece na tela, se o app aberto continua rodando e se reinstalar por cima preserva identidade, servidores salvos e pinos [A14][K4]. Ver também se a primeira abertura precisa de internet, numa LAN sem saída [A15].

**Para amigos, hoje:** se eles só precisam **entrar** em servidores, espere o resultado do spike e não faça nada ainda. A rota C escala sem pagar. Se precisam do app nativo agora, a rota B serve só para um amigo técnico que aceite o risco: bans [B11], login quebrado [B10], identidade nova a cada troca de ferramenta ou conta [B13] e a §2.4 [A3]. Não publique uma fonte `.ipa` pública sem o programa.

**Quando os US$ 99 (R$ 549,90/ano [D9]) passam a valer a pena**
- **Se o spike falhar no iPhone**, por SETTINGS ou por voz que morre com a tela bloqueada. Aí a única rota grátis para terceiros some.
- **Quando houver amigos usando toda semana.** O suporte da rota B (renovação, bans, "A CHAVE DO SERVIDOR MUDOU") custa mais que a anuidade. A AltStore PAL resolve para quem está no Brasil, sem expirar [D6].
- **Quando o Developer ID do macOS também for necessário.** É a mesma conta, e a meta do APOIAR já aponta para ela [D22]. O GitHub Sponsors pode pagar a anuidade sem taxa [D15].
- **Antes de pagar, confira três coisas:**
  - inscrever-se como **pessoa física** converte o Personal Team no time individual. Como **organização**, o Personal Team continua ao lado do time novo, e o bundle ID pode ficar "not available" no time novo até expirar no Personal Team [A13][A12];
  - a falta de LICENSE e o registro "Seele" na classe 9 da EUIPO podem travar a revisão pela guideline 5.2.1 [D22];
  - pagar **não** resolve servidor em segundo plano, multicast (continua sendo pedido à Apple) nem CGNAT.

---

## 6. O que ficou incerto e precisa de confirmação

**Medir no aparelho**
- O comportamento quando o perfil vence: app já aberto, mensagem na tela, dados após reinstalar [A1][A14].
- O check-in com ppq.apple.com na primeira abertura de app do Personal Team [A15].
- A falha do UPnP e o contorno unicast, o PCP no iOS, e a sobrevivência do socket que escuta depois de suspensão ou interrupção [K1][A17].
- Se o `devicectl` instala por Wi-Fi com o iPhone bloqueado, e quanto dura a sessão da conta no Xcode antes de pedir 2FA [A11].
- Se o `cargo tauri ios build --no-sign` entrega um `.ipa` [K6].
- Na rota C:
  - se o wtransport e o Safari conversam [C17];
  - se o `serverCertificateHashes` funciona de fato no iOS, já que o MDN só espelha o macOS, e se aceita listas longas de hashes [C3][C5];
  - o Opus no WebCodecs do iPhone;
  - o microfone com a tela bloqueada, na aba e no web app [C11][C12];
  - se o Web Push ainda exige `display` standalone no iOS 26+;
  - se o iCloud Private Relay interfere no WebTransport.
- Se o SEELE roda dentro do LiveContainer no iOS 27 [B16].
- Se a renovação do SideStore ou do AltStore derruba um servidor em execução [B10].

**Confirmar com a Apple ou em fonte primária**
- Se o Personal Team pode obter o entitlement de multicast. É inferência forte, mas não está escrita [A8].
- Se a regra 1.2 entra na notarização. As duas verificações divergiram [D7].
- Se o Brasil ainda é elegível à isenção, e que documento brasileiro a Apple aceita [D10][D11].
- Se o banimento atinge perfis emitidos pelo Xcode, a sua rota atual, e qual é a causa [B11].
- Se o limite de 3 apps é por time ou por aparelho, e quanto tempo um App ID leva para liberar o bundle ID [A1][A12].
- Se o Xcode 26 num Mac Intel instala num iPhone com iOS 27, para amigos sem Apple silicon [A10].
- Se o preço de R$ 549,90 vale também pelo site, e com quais impostos [D9].
- Se, na rota de publicar sob uma organização parceira, o DPLA exige que ela seja a dona do app [D8].

**Ferramentas de terceiros**
- A data real do AltStore Classic 2.3: 14/09 nas notas, 30/09 no GitHub [B1].
- Se a AltStore PAL é gratuita para quem instala em 2026 [B7].
- Se o AltStore Classic instalado pela PAL ocupa uma das 3 vagas.
- Quem opera os Remote AltServers [B4].
- Se o Developer Mode aparece num iPhone que nunca pareou com um Mac [A9].
- Se existe versão estável do SideStore e do LiveContainer validada no iOS 27 final [B10][B16].

**Regulatório**
- O texto integral do TCC e por que as lojas abriram cerca de 2 meses depois do prazo de 105 dias [D4].
- A situação do PL 4.675/2025, sem notícia desde agosto de 2026.
- A regra de 90 dias fora da UE depende do iOS 27.2, ainda em beta [D20].
- Não há dado de CGNAT em rede celular no Brasil em 2026.

**Divergências no código** (de passagem, independentes da rota)
- `sessao-de-audio.m` diz "modo VoiceChat" no comentário, mas o código usa `AVAudioSessionModeDefault`.
- O comentário de `hospedagem.rs:105-110` diz que o degrau 4 depende de `$SEELE_ENCONTRO`, mas `encontro.rs` usa o ponto de encontro padrão sem nenhuma variável.

**Fontes do código e das medições locais** (lidas em 2026-10-07, branch `mobile/ios`, HEAD `3685a64` mais a árvore sem commit)
- [K1] `crates/seele-server/src/alcance/porta.rs` (linhas 58-70, 125-131, 178-182, 310-321), `alcance/pcp.rs`, `alcance.rs` (1153-1205) e igd-next 0.17.1 `src/common/options.rs`.
- [K3] `apps/seele-app/src/lib.rs` (updater registrado sem cfg em 8727; `abrir_versao` e `baixar_versao`), `crates/seele-lancador/src/inicializacao.rs:72-78` e tauri-plugin-updater 2.10.1 `updater.rs`.
- [K4] `apps/seele-app/src/lib.rs:8520-8552` (`pasta_no_ios`).
- [K5] `tools/ios-aparelho.sh` e `apps/seele-app/tauri.conf.json:5`.
- [K6] tauri-cli 2.11.4 (strings do binário e `cargo tauri ios build --help`).
- [K7] `git log main..HEAD` dá 0 commits; `git status` mostra 21 arquivos modificados e 14 sem rastreio.
- [K8] https://github.com/DATA-AND-DEV/SEELE-RELEASES/releases/expanded_assets/v0.15.0. Lido em 2026-10-07: sem `.ipa`.
- [K9] `xcrun devicectl list devices` em 2026-10-07: o iPhone de teste `unavailable`.
- [K10] `specs/06-clientes-gui.md:111-125`, `specs/09-roadmap.md:97-103` e `docs/adr/README.md:96`.
- [K11] `apps/seele-app/gen/apple/project.yml` (versão 0.0.0) e `.github/workflows/release.yml` (sem job de iOS).