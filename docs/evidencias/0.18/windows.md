# Windows 0.18 — validação isolada

28/10/2026. Autorização humana explícita: “Autorizar envio e testes da0.18”, para o delta privado em `aleta@192.168.50.30:C:\s17v1025\src`, cache existente `C:\s17v1025\t`; sem instalação/publicação, firewall ou alterações em produção.

## Resultado comprovado

- Delta único: 172 arquivos, 3.154.933 bytes, SHA256 `4503526022aa745e222f4be85f99f39159c258eecdd91c06239c15eae32c9567`.
- Snapshot: 873 arquivos governados, manifesto SHA256 `ce1b386d905602ccd13107f0a36a7331952ba3ce9376c403325b86a855e15cdf`; todos conferidos no Windows antes e depois do build. WASM de produção 2.215.066 bytes, proveniência `SEELE_VERSAO=0.18.0` conferida no Mac antes do envio.
- Scripts passaram no AST Parser do PowerShell antes de executar. Um erro inicial do verificador (diretório raiz `ui` opcional ausente) bloqueou antes de Cargo; guarda corrigida e AST repetido.
- `cargo +1.97.1 build --locked -p seele-app --bin seele-app --jobs 2`: PASS, debug, 2m04s, cache existente; apenas aviso do linker sobre `.lib/.exp`.
- `cargo +1.97.1 test --locked -p seele-app --test frontend --jobs 2`: **348 passaram**, zero falhas.
- EXE: `C:\s17v1025\t\debug\seele-app.exe`, 92.573.184 bytes, SHA256 `fc7fe0c76e3931519e91b477a23d4c08dd457478c6bd86a41701b0daf625cf72`.
- WebView2 real **156.0.4314.50**: DOM, WASM pessoal e controles a 1280 px sem overflow passaram em duas aberturas.
- IPC Tauri real `pessoal_identidade`/`meu_codigo`, sem ponte fixture: código **NN3Q-1VF1-G6H2-K365**, SHA256 da chave pública `ad4770ede181a2298cd21b0dff9f81066e4e09033d71a3961ddd67d66a8c2248`, idênticos após fechar e reabrir o mesmo perfil temporário.
- Perfil: `C:\s17v1025\perfis\primeira-abertura-018-20261028-163745`. Preferência **`receber_ligacoes\tnao` preconfigurada antes da primeira carga**, além de `SEELE_ENCONTRO=nao`. IPC confirmou `receber=false`; zero recursos `rendzvous.seele.sh` no Performance API da página nas duas provas aceitas.
- CDP9334 escutou exclusivamente `127.0.0.1`; zero regras de firewall para a porta. Runner existente isolou `SEELE_HOME` e `WEBVIEW2_USER_DATA_FOLDER`, encerrou sua árvore/tarefas e confirmou perfil real e `C:\SEELE` sem mudança. App já encerrado após a prova.

[Resultado IPC](windows-ipc-persistencia.json) · [Captura real da reabertura](windows-reabertura.png)

## Rodada inicial reprovada, preservada

O primeiro perfil vazio foi aberto com `SEELE_ENCONTRO=nao`, mas sem preferência prévia. O produto usa `Preferences::receber_ligacoes().unwrap_or(true)`: meu teste supôs falso e reprovou o assert de identidade/recepção. DOM/WASM/controles passaram, mas essa rodada **não comprova isolamento da recepção JS**. A variável de encontro desliga o caminho nativo; a configuração canônica do serviço JS continua apontando ao domínio público quando a recepção está habilitada. Não foi possível comprovar a ausência de tentativa/registro público nessa rodada; não é tratada como PASS. O perfil era inteiramente descartável, e sua árvore foi encerrada. Log preservado em `/private/tmp/seele-018/windows/evidencias/primeira-rodada-reprovada.log`.

As duas rodadas aceitas corrigem a preparação do teste, sem alterar o produto: preferência `nao` gravada antes de abrir e confirmada via IPC real.

## Proveniência, localização e limites

Logs remotos: `C:\s17v1025\validacao-018\rodada-20261028-162859\{build-app.log,frontend.log,resultado.json}`. As duas rodadas de UI geraram logs `primeira-abertura-20261028-163746.log` e `primeira-abertura-20261028-163842.log`, capturas e conferência do perfil real. Cópias de resultados/logs/capturas e scripts em `/private/tmp/seele-018/windows/` (não contém chave privada nem cópia do perfil).

A persistência das duas aberturas acima corresponde exatamente ao manifesto/EXE inicial. A rodada final e a interoperabilidade após o ajuste JS estão registradas abaixo. Não foram testados instalador, updater, assinatura ou publicação.


## Rodada final após o ajuste JS — PASS

- Delta incremental sobre o snapshot já aprovado: três arquivos, 14.782 bytes; SHA256 `2ee498b80421f55f07069f3712fb3d7b0c75dab761fa7a500369b4e6916f88cd`. Inclui o ajuste `pessoal-web-entrada.js` e duas bancadas. Manifesto governado de 873 arquivos: `1702895ea4ff08f8bc329ddcf4eb55181f9959a6d6280366d5debfffcc83c871`; todos os hashes conferidos antes/depois do build. WASM de produção com proveniência `SEELE_VERSAO=0.18.0` mantida.
- Build isolado e testes frontend novamente PASS: **348 testes**, zero falhas. Rodada `C:\s17v1025\validacao-018\rodada-20261028-171836`. EXE final: 92.573.184 bytes, SHA256 `6ec5bc3da858ccb55fb0383bca733a2dfb39f06618f22f251ab5c83d1afdfb93`.
- WebView2 **156.0.4314.50** com IPC Tauri real, identidade/assinatura/reserva nativas, conectado a Edge headless isolado no mesmo Windows. Duas chamadas: navegador chama o código Windows e Windows atende; depois Windows chama o código navegador e Edge atende. Texto enviado e recebido nas duas pontas, em cada chamada. Encerramento e desligamento da recepção concluídos.
- Áudio sintético silencioso passou pelo codificador/decodificador e transporte do produto: amostra positiva preservada antes de encerrar, **Windows enviados 11 / decodificados 16; Edge enviados 16 / decodificados 12**. Zero falhas de voz; track sintética `ended` no fim. Nenhum microfone ou som ambiente foi capturado; elementos de áudio ficaram mudos.
- Perfis nativos sempre criados com `receber_ligacoes\tnao` antes da primeira carga, e `SEELE_ENCONTRO=nao`. Recepção habilitada depois, via comando IPC real, somente após instalar a sinalização controlada. Zero URLs públicas capturadas pelo controlador na rodada final; CDP9334/9335 conferido exclusivamente em `127.0.0.1`, sem regras de firewall.

[Resultado final completo](windows-interop.json) · [Log final exit0](windows-interop.log) · [WebView2 real](windows-interop-native.png) · [Edge real](windows-interop-edge.png)

### Escopo e correções da fixture

O encontro Rust roda no loopback Mac; as requisições de sinalização na origem de cada página são encaminhadas pelo CDP a esse encontro. A factory da entrada JS recebe somente overrides de encontro local/origem local e `iceServers=[]`. O browser carrega a UI/WASM de produção; assets do AudioWorklet são servidos por HTTP em loopback através de SSH reverso, pois sua carga não foi atendida pela rota CDP. CSP permaneceu ativa. Nenhuma mudança de produto foi feita para permitir a prova.

As tentativas intermediárias não são provas de áudio: primeiro o snapshot foi coletado sem reter a amostra positiva; depois o diagnóstico capturou `AbortError: Failed to load worklet module script` no Edge porque a origem HTTP da fixture ainda não tinha servidor real. Uma fonte sintética também precisou de stop idempotente e o controlador passou a tolerar a carga inicial adicional da janela. O resultado final usa polling explícito com `await page.evaluate(...)`, guarda a amostra positiva e exige contadores enviados/decodificados maiores que zero nas duas pontas. Esses erros de fixture não foram atribuídos ao produto nem à biblioteca de automação.

Esta prova valida WebView2↔Edge no **mesmo host Windows** com RTC real e sinalização controlada. Não valida sinalização de produção, NAT entre máquinas, internet, captura de microfone real, tela/vídeo no Windows, instalador ou atualização.


### Limpeza concluída

O runner final encerrou os nove processos de sua árvore e removeu suas duas tarefas; log confirmou **perfil real e `C:\SEELE` sem mudança**. O cleanup encerrou somente os dez processos do Edge do perfil de teste, removeu sua tarefa interativa e os seis perfis descartáveis criados nesta prova (incluindo identidades temporárias). CDP9334/9335 fechados, túneis SSH de CDP/reverso e servidor de assets Mac encerrados. Logs pequenos preservados; nenhuma instalação/publicação ou regra de firewall foi feita.

[Build final](windows-final-build.json) · [Cleanup](windows-interop-cleanup.json) · [Log do runner final](windows-interop-runner.log)
