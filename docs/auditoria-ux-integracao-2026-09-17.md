# SEELE — auditoria de experiência e integração para o Claude

Data: 17/09/2026. Base examinada: `b3076d930356aaa49ffdb3f2bfe0d99f1718bd7c`.

## Mandato e limites

Pedido: diagnosticar o Mesa instalado que não aparece, procurar falhas e etapas repetidas no SEELE, testar graficamente e entregar este documento **sem corrigir a implementação**.

Esta é uma auditoria transversal inicial, não uma certificação de todo o projeto. Foram examinados os fluxos de entrada, hospedagem, reconexão, configuração, MODs, catálogo, consentimento, partes do servidor/core relacionadas, testes e pendências. Não houve revisão linha a linha de todos os crates nem auditoria completa de segurança, codecs ou transporte.

Nenhum código, preferência, MOD instalado, aceite ou servidor foi alterado por esta auditoria. Os testes automatizados produziram seus artefatos normais em `target/` e diretórios temporários. A navegação gráfica terminou na tela inicial; não foi iniciado servidor, enviada mensagem, aberta transmissão nem feita instalação. Este Markdown é a única entrega no repositório.

Classificação das evidências:

- **G — gráfico:** observado no aplicativo nativo macOS, por imagem e árvore de acessibilidade.
- **C — código:** caminho identificado na base acima; não necessariamente reproduzido no aplicativo instalado.
- **L — log:** evento histórico da instalação local, não uma reprodução feita nesta auditoria.
- **H — hipótese:** investigação necessária antes de afirmar causa ou implementar remendo.

Prioridades: P1 bloqueia fluxo principal ou pode encerrar serviço; P2 prejudica uso/recuperação; P3 clareza/acabamento. Não foi confirmado incidente crítico de segurança.

## Ambiente e resultado geral

O aplicativo inspecionado foi `/Applications/SEELE.app`, com origem visual `tauri://localhost`. A seção VERSÕES exibe **0.11.0**. Os logs mostram cliente 0.11.0 e protocolo 6. O Mesa instalado é **seele/mesa 1.2.0**.

A consulta pública à API de releases retornou `v0.10.5-1`, publicada em 05/09, baseada em `12a6401a617a8313f93a32e3421dea58b29cdde9`. Isso **não prova** que o aplicativo local esteja desatualizado: sua própria interface e seus logs indicam 0.11.0. Também não se comprovou que o binário instalado corresponda exatamente ao HEAD auditado. Conferir artefato/commit antes de atribuir diferenças a publicação. Fonte: <https://api.github.com/repos/DATA-AND-DEV/SEELE-RELEASES/releases?per_page=3>.

No início do teste gráfico, o aplicativo estava **desconectado e sem hospedar**. Os registros de aceite/conexão do Mesa são de momentos anteriores. Portanto, a ausência do botão MESA nessa tela inicial não reproduz, por si só, a falha de carregamento durante uma sessão ativa.

Principais resultados:

1. Convite inválido não apresenta erro no diálogo: IDs HTML duplicados direcionam a mensagem à tela errada.
2. Preparação de servidor mostra literalmente `NaN KiB, undefined px`.
3. Reconectar reaproveita uma limpeza que encerra o servidor hospedado.
4. A ativação de MOD e o consentimento local são fluxos separados, fazendo o host responder novamente; a tentativa seguinte ainda perde o contexto de hospedagem.
5. Erros de carregamento de MOD ficam no console, não na interface.
6. O carregamento por protocolo customizado precisa ser validado em WebKit/WebView2; o uso de JavaScript compartilhado não basta para prometer compatibilidade nativa.

## Achados

### A01 — P1 · Convite inválido falha sem mensagem no diálogo [G+C]

**Reprodução:** CONECTAR → digitar `seele://` → ENTRAR. O diálogo continua aberto, sem mensagem visível. Não foi feita conexão a servidor remoto. O envio vazio também não dá feedback, mas é um caso separado: o handler retorna explicitamente.

**Causa identificada:** `index.html` declara duas vezes `servidores-erro`, nas linhas 421 e 3091, e duas vezes `servidores-titulo`, nas linhas 408 e 3074. `base.js:39` usa `document.getElementById`; `recusarServidor`, em `camada-servidores.js`, escreve no primeiro erro, que pertence à tela de hospedagem escondida. O mesmo conflito faz a árvore de acessibilidade identificar o diálogo ONDE VOCÊ JÁ ESTEVE como **HOSPEDAR**, também observado graficamente.

**Direção:** IDs distintos por superfície e revisão de todas as referências. Para campo vazio, desabilitar ENTRAR ou apresentar instrução/foco no campo.

**Aceite:** convite malformado mostra erro no próprio diálogo; leitor de tela anuncia o título correto; nenhum ID duplicado no HTML efetivo; nenhuma tentativa de rede para convite sintaticamente inválido.

### A02 — P2 · Limites de imagem aparecem como NaN e undefined [G+C]

**Reprodução:** HOSPEDAR AQUI → NOVO SERVIDOR. Texto observado: `Até NaN KiB, undefined px de lado. Uma foto maior é reduzida aqui mesmo.` Não foi necessário selecionar imagem ou criar servidor.

**Causa:** `tela-boot.js:757` (`abrirPreparar`) lê `regras.teto_em_bytes` e `regras.lado_maximo`; `main.rs:1725` e `main.rs:1782` serializam `limite_bytes` e `lado`. A configuração já utiliza os nomes corretos em `tela-server.js`.

**Direção:** alinhar contrato, validar resposta antes de interpolar e apresentar fallback honesto se o IPC falhar.

**Aceite:** limites numéricos reais em preparação e configuração; teste usa o JSON serializado pelo Rust e verifica o texto renderizado, não só presença do nome do comando.

### A03 — P1 · RECONECTAR encerra a própria hospedagem [C]

**Caminho:** `tela-fim.js:121` → `limparSessaoEncerrada` (`:157`) → `invoke("disconnect")`. Em `main.rs:1173`, `disconnect` recolhe a conexão **e** a hospedagem, executando `server.encerrar().await`. Depois, `reconectar` chama `conectar()` com o endereço anterior, sem levantar novamente o servidor.

**Impacto:** quando a sessão local do host termina, inclusive por mudança de MODs, o botão que promete reconectar pode desligar o servidor e tentar entrar num endereço que ele próprio acabou de tirar do ar. Não é a mesma intenção que SAIR/ENCERRAR HOSPEDAGEM. Não reproduzido graficamente para não derrubar serviço nem iniciar hospedagem nos dados reais.

**Direção:** separar desligamento de cliente e encerramento de hospedagem. Reconectar o cliente local deve preservar servidor, porta, banco escolhido e demais participantes. Caso se escolha reiniciar o servidor, isso precisa de ação explicitamente nomeada e explicação do impacto.

**Aceite:** com host e convidado conectados, terminar apenas a sessão local e usar RECONECTAR não desliga a escuta nem derruba o convidado. Testar também `ModsMudaram`. SAIR de verdade continua com a semântica prevista e avisada.

### A04 — P2 · Host decide ativar e depois precisa aceitar de novo [C+L; relato do usuário]

**Por que acontece:** `habilitar_mod` (`main.rs:2021`) grava os MODs exigidos no banco do servidor, mas não registra consentimento no cliente local. `session.rs:2995` encerra conexões cujo conjunto aceito mudou, sem distinguir o operador que fez a alteração. Na próxima entrada, a interface genérica de aceite é aberta. `aceitarOsMods` (`camada-mods.js:157`) grava o aceite e chama `conectar()` novamente.

Instalar, ativar no servidor e consentir com execução no cliente são decisões tecnicamente diferentes. Mas o produto expõe essa separação interna como etapas repetidas para a mesma pessoa, sem explicar o ganho. **Não recomendar uma isenção genérica para qualquer host ou administrador.** Um servidor pode ser alterado por outro processo/operador, e conceder confiança ilimitada eliminaria uma proteção real.

**Fluxo recomendado:** instalar continua sendo copiar sem executar; quando o operador local escolhe ativar, uma única ação informada, como “ATIVAR NESTE SERVIDOR E USAR NESTE COMPUTADOR”, explica alcance e efeito sobre os participantes. Essa ação registra o consentimento local somente para o conjunto exato resultante e para a identidade correta do servidor. Convidados continuam aceitando em seus dispositivos. Mudança posterior por terceiros ou troca de conteúdo exige nova decisão.

**Aceite:** uma decisão informada do operador local por conjunto; nenhum segundo modal equivalente; nenhum bypass baseado apenas em `localhost`, apelido ou papel de administrador; alteração concorrente do conjunto não herda consentimento indevido.

### A05 — P2 · Aceitar os MODs perde o contexto de “meu servidor” [C]

`tela-boot.js:148` captura e zera `subindoServidorAqui` antes de tentar conectar. Se a tentativa para no aceite, `aceitarOsMods` chama `conectar()` sem preservar a intenção original. `entrarNaAutenticacao`, em `tela-auth.js`, só passa direto quando recebe `nossoServidor` ou quando vinha da espera de admissão.

**Consequência prevista:** o host que deveria pular a conferência da própria hospedagem pode cair na tela ENTRAR NO SERVIDOR depois do aceite. É outra etapa repetida, distinta do A04. A intenção local foi consumida numa tentativa que não concluiu.

**Direção:** representar a operação de entrada como contexto identificável, preservado apenas durante a continuação do mesmo alvo. Não manter uma flag global verdadeira que possa vazar para a próxima conexão remota.

**Aceite:** hospedar com MOD ainda não aceito → aceitar → sessão, sem autenticação redundante; cancelar e escolher servidor remoto não herda o atalho local.

### A06 — P1 · Mesa invisível não tem diagnóstico voltado ao usuário [C+L; causa raiz ainda H]

**Evidência histórica:** `seele.log` registra Mesa 1.2.0 anunciado, recusa por `ModsNaoAceitos`, aceite e entrada com protocolo 6. Em 17/09 às 23:07:25 UTC há aceite seguido de entrada. Isso não demonstra execução do JavaScript do cliente.

**Evidência atual:** Configurações → MODS lista Mesa 1.2.0; sem hospedar, LIGAR está corretamente desabilitado e explicado. O cliente do Mesa cria o botão MESA, no canto inferior direito, quando seu script executa. Instalação não substitui a tela inicial.

**Defeito comprovado por código:** `base.js:443–499` distingue pacote ausente/hash divergente/falha de catálogo/falha no script, mas comunica esses casos apenas por `console.warn/error`. A gestão mostra instalação e ativação, não “carregado”, “falhou” ou “incompatível”. O usuário fica sem próximo passo.

**Hipótese prioritária:** o loader cria `script.type = "module"` com `mod://localhost/...`; o handler `main.rs:4044` fornece Content-Type, mas não define cabeçalho CORS na resposta. Conferir rejeição por origem/protocolo no console do WebKit antes de atribuir a ausência do Mesa a isso. Não foi capturado erro do WebView que confirme esta causa. Conferir também resposta real do catálogo, igualdade do hash, resposta do protocolo e exceção de inicialização do MOD.

**Direção:** diagnóstico visível por MOD, fase e erro; manter última falha com opção de tentar novamente; distinguir bytes carregados de inicialização concluída. Persistir eventos úteis no log nativo, sem despejar dados privados da campanha.

**Aceite:** provocar hash divergente, script ausente, exceção de inicialização e recusa de catálogo; cada caso deve exibir estado e ação útil. No caminho feliz, botão MESA abre campanha no app empacotado. Não considerar o preview HTTP do Mesa como teste desta integração.

### A07 — P1 · URL/CSP de MOD precisa de tratamento multiplataforma [C+H]

`base.js:487` monta `mod://localhost` diretamente. `tauri.conf.json` permite `mod:` em `script-src`. A documentação oficial do Tauri descreve protocolos customizados em Windows/Android com origem `http://<scheme>.localhost` ou HTTPS conforme configuração. É um risco concreto de integração, não prova de falha já observada no Windows.

**Direção:** gerar URL pelo mecanismo de conversão compatível com o Tauri usado; conferir esquema, origem, CORS e CSP em cada plataforma. Não resolver liberando `unsafe-inline`, `unsafe-eval`, origens arbitrárias ou enfraquecendo a validação de hash/caminho.

**Aceite:** o mesmo pacote Mesa abre no SEELE empacotado de macOS e Windows, com CSP restritiva. Registrar URL efetiva, status e inicialização. Fazer regressão de traversal, arquivo não declarado e hash errado.

Referências técnicas: [protocolos no Tauri](https://v2.tauri.app/reference/config/), [módulos JavaScript e CORS](https://developer.mozilla.org/en-US/docs/Web/JavaScript/Guide/Modules), [configuração CORS](https://developer.mozilla.org/en-US/docs/Web/Security/Practical_implementation_guides/CORS). São fundamento para a investigação; não substituem reprodução no WebView nativo.

### A08 — P2 · LIGAR/DESLIGAR não avisa que altera entrada e derruba sessões [C]

`trocarOMod` (`camada-mods.js:274`) invoca a alteração diretamente. `ModNaTela` fornece `exigencia_vale_na_rede`, mas o painel não o utiliza. A alteração pode terminar sessões por `ModsMudaram` (`session.rs:2995`). A pendência #44 já descreve o problema: não é achado novo, e continua relevante no fluxo examinado.

**Direção:** explicar a consequência antes da alteração e permitir preparar várias mudanças para uma única aplicação, se esse comportamento for adotado pelo produto. Não apresentar desligar como uma ação sem impacto só por parecer menos arriscada que ligar.

**Aceite:** usuário sabe quais participantes precisarão reconectar e aceitar; múltiplas alterações não viram uma sequência evitável de quedas e aceites. Testar host e convidado separadamente.

### A09 — P2 · Catálogo não oferece atualização quando o ID já existe [C]

`camada-mods.js:473` usa somente `i.id === mod.id` para mostrar JÁ INSTALADO. Versão/hash/recusa local não entram na decisão. `mods.rs:221` também recusa qualquer destino existente. Assim, instalar a mesma identidade é protegido contra sobrescrita, mas não há caminho de atualização nessa tela.

**Impacto:** cliente com versão diferente da exigida pelo servidor fica com pacote incompatível e sem ação correspondente no catálogo; reinstalar por pasta também recusa. Não remover dados manualmente como solução normal.

**Direção:** exibir versão instalada versus oferecida, hash exigido pelo servidor e ação de atualização/reparo segura. Separar pacote de dados, planejar rollback e invalidar consentimentos apenas conforme o conteúdo resultante.

**Aceite:** atualização mantém campanhas; falha preserva pacote anterior; pacote recusado não aparece como instalação saudável; versão exigida é explicitamente identificada, sem presumir que “mais nova” serve para todo servidor.

### A10 — P2 · Instalação por pasta pode deixar destino parcial e impedir nova tentativa [C]

`mods.rs:212–249`: valida origem, recusa destino existente, cria/copía diretamente para o destino final e propaga erro sem rollback. Se houver erro de leitura/escrita depois de criar a pasta, a próxima tentativa encontra `JaInstalado`. O achado trata de falha de I/O durante a cópia; não depende de afirmar que um pacote inválido passou pela validação.

**Direção:** copiar para staging exclusivo, validar resultado e publicar atomicamente; limpar somente o staging criado pela tentativa. Nunca apagar instalação preexistente nem dados de campanha para recuperar erro.

**Aceite:** injetar falha após a primeira escrita; instalação final anterior permanece intacta ou inexistente; repetir funciona; nenhuma pasta parcial se apresenta como instalada. Os testes atuais de recusa e de não sobrescrita não cobrem essa sequência.

### A11 — P2 · Sucesso da instalação do catálogo é sobrescrito pela nova busca [C]

`instalarDoCatalogo` (`camada-mods.js:531`) escreve “instalado, e desligado”, redesenha e chama `buscarOCatalogo` novamente. Essa busca escreve “buscando…” e depois a contagem; se a rede falhar após instalar, o usuário pode ler erro de catálogo como se a instalação tivesse falhado. O comentário diz redesenhar, mas há uma nova consulta de rede.

**Direção:** redesenhar a lista já disponível após sucesso e manter resultado da instalação separado do estado da consulta. Revisar cliques repetidos: o botão por item não é desabilitado durante o download.

**Aceite:** sucesso permanece visível mesmo se catálogo ficar indisponível imediatamente depois; clique duplo não inicia duas instalações; erro de atualização da lista não apaga o resultado da operação concluída.

### A12 — P3 · A tela inicial afirma “porta udp aberta” sem hospedar [G+C]

A frase está fixa em `index.html:300`, junto de `$ seeled 0.0.0.0:8383` e “pronto.”. Foi observada quando a gestão de MODs indicava não hospedar. Isso não demonstra que uma porta esteja fechada: demonstra que a interface faz a afirmação sem medir esse estado.

**Direção:** distinguir apresentação do produto de status real. Usar “pronto para conectar ou hospedar” na entrada; mostrar estado de escuta/alcance somente quando apurado.

**Aceite:** nenhuma promessa de conectividade antes da medição; hospedagem em rede local não é apresentada como acessível pela internet.

### A13 — P3 · Ajuda de atalhos contradiz o controle editável [G+C]

Configurações → ATALHOS mostra “Estas teclas não mudam.” e logo abaixo um botão “Falar enquanto segura: ESPAÇO. Clique para trocar”. `index.html:2455` contém a afirmação geral.

**Direção:** dizer que a tecla de falar é personalizável e separar atalhos fixos. Também revisar nomes de modificadores por plataforma; não foi testada a colagem de imagem nesta rodada.

**Aceite:** a explicação corresponde aos controles e à plataforma, sem precisar alterar uma preferência para entender a regra.

### A14 — P2 · Testes verdes não verificam vários contratos renderizados [G+C+execução]

Executado `cargo test -p seele-app --test frontend`: **191 passaram, zero falhas**. Ainda assim A01 e A02 foram reproduzidos no aplicativo, e seus defeitos existem no código auditado. `frontend.rs:10506` confere texto da CSP; `:10527` confere strings/ordem do loader, não sua execução no WebView.

Executado `cargo test -p seele-app --bin seele-app mods::`: **9 passaram, zero falhas**. Cobrem instalação/serviço em Rust; não provam que um módulo JS atravessa origem, CSP, IPC e inicializa o Mesa no aplicativo.

**Direção:** manter guardas estáticos, acrescentar unicidade de IDs, contratos de IPC serializados e testes de comportamento com DOM real; para protocolos customizados, manter smoke test no binário nativo por SO. Não substituir tudo por testes que apenas verificam se a correção está escrita.

**Aceite:** testes falham com A01/A02 presentes; suíte de integração prova abertura do MOD e caminho de recuperação, não só existência do script.

### A15 — P2 · Registro de pendências já diverge da implementação [C]

`docs/pendencias.md` ainda afirma na #45 que `EnterVoiceRoom` não verifica `Permission::EnterVoiceRoom` e que a expulsão é desfeita pela reconexão automática. O código atual verifica a permissão em `session.rs:1850`; `enlace.rs:292` trata Kicked/Banned/ModsMudaram como término. Também há texto dizendo que catálogo não está disponível, embora existam integração e painel.

**Não concluir** que toda a #45 está resolvida: ela descreve outras condições e precisa de teste de campo. O problema documentado aqui é usar o texto histórico como fotografia confiável do estado atual.

**Direção:** atualizar status com commit, evidência e o que continua pendente, preservando numeração. Separar “implementado”, “testado” e “publicado”.

**Aceite:** cada pendência ativa continua reproduzível ou está claramente marcada como hipótese/validação restante; não reimplementar um conserto já existente com base em documentação antiga.

## Teste gráfico realizado

Aplicativo nativo macOS, não um mock da interface. Capturas e árvores foram observadas nesta conversa; não foram exportadas imagens com dados da sessão para o documento.

| Fluxo | Resultado |
|---|---|
| Entrada → configurações → voltar | Funcionou; voltou à entrada |
| Microfone e som | Dispositivos e explicação de padrão visíveis; sem alterar aparelho ou abrir captura |
| MODs instalados | Mesa 1.2.0 visível; sem hospedagem, LIGAR desabilitado e explicado |
| Atalhos | Contradição de texto A13 reproduzida |
| Versões | 0.11.0 identificada; não foi baixada outra versão |
| Atualização | Painel abre; não foi acionada instalação |
| CONECTAR → ENTRAR vazio | Sem mensagem ou validação visível |
| CONECTAR → `seele://` → ENTRAR | Sem erro no diálogo, A01 |
| CONECTAR → Escape | Diálogo fecha |
| HOSPEDAR AQUI → seleção | Lista de servidores existente abre, sem iniciar nenhum |
| NOVO SERVIDOR → preparação | A02 reproduzida |
| Preparação → VOLTAR | Retorna à entrada, sem criar servidor |

O VOLTAR da preparação pula a lista de servidores da qual se veio. É uma observação de navegação, não classificada como defeito sem decidir a intenção do produto; considerar restaurar a origem se a pessoa estava escolhendo entre servidores.

## Cobertura restante — necessária antes de chamar isto de auditoria completa

Não foram testados graficamente nesta rodada: sessão ativa, criação de campanha, chat/anexos/busca, salas de voz, áudio ouvido por outro participante, compartilhamento de tela, portaria com outro dispositivo, moderação real, instalação/atualização/ativação de MOD, reconexão do host, Windows/Linux, mudança de dispositivo, NAT/firewall, processo de publicação e update completo. Não há resultado “aprovado” para esses itens.

Para avançar, usar perfil isolado (`SEELE_HOME` é suportado em `main.rs:233`) e portas/servidores de teste, sem copiar identidade, convites, aceites ou campanhas do usuário. Executar o mesmo binário que será distribuído. Não foi criada nem iniciada uma segunda instância nesta auditoria.

Matriz proposta:

1. **MOD/host/convidado:** instalar sem ativar; ativar com host e convidado; consentimento exato; recusar; atualizar versão; desligar; reconectar; reabrir o aplicativo.
2. **Mesa:** botão visível; criar campanha; GM/jogador; ficha/retrato; cena/mapa; iniciativa; permissões; música com opt-in; fechar/desconectar encerra áudio e timers.
3. **Entrada e navegação:** primeiro acesso, retorno, convite inválido/expirado, aprovação/recusa, mudança de versão, cancelar durante conexão, Escape, voltar e foco de teclado. Validar que só a tela pretendida e suas camadas estão ativas.
4. **Chat:** texto, mensagem longa, imagem, anexo expirado, busca, transferência interrompida, erro de disco, histórico após reconectar; verificar feedback sem expor conteúdo privado em logs.
5. **Voz/tela:** entrar/sair, senha/permissão, troca de aparelho e padrão do SO, aparelho removido, espectador sai/volta, fonte deixa de existir, permissão de captura recusada. Exige segundo dispositivo e escuta real.
6. **Moderação/portaria:** expulsar não reconecta automaticamente; banir; aprovação enquanto janela minimizada; alteração de papel; sessão antiga concorrente. Revalidar pendências com código atual.
7. **Distribuição:** Mac Intel/Apple Silicon e Windows suportados; download interrompido; assinatura/revogação; versão incompatível; recuperação de update; confirmar artefato/commit. A existência de workflow não comprova que uma execução passou.

## Ordem recomendada para o Claude

1. Confirmar base/artefato e reproduzir A01/A02; são correções pequenas com evidência gráfica forte.
2. Reproduzir A03 em ambiente isolado antes de mudar reconexão.
3. Tratar A04/A05/A08 como um único desenho de fluxo, preservando consentimento e separação host/cliente.
4. Capturar falha real do Mesa no WebView e resolver A06/A07 com smoke tests nativos.
5. Completar atualização/recuperação do catálogo e instalação (A09–A11).
6. Fechar mensagens, acessibilidade, cobertura e documentação (A12–A15).

Para cada alteração futura: registrar reprodução anterior, teste que falha antes e passa depois, efeitos sobre host/convidado e plataformas efetivamente testadas. **Este documento não contém correções aplicadas e não é evidência de homologação do Mesa dentro do SEELE.**
