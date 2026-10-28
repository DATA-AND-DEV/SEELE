# Links pessoais e erros de entrada — JavaScript 0.18

Validado em 28/10/2026, sem Cargo, publicação ou áudio físico.

`chamada-web.js::abrirConvite` aceita URL absoluta somente na origem exata de `origemDosLinks`, incluindo esquema e porta. Aceita também `#chamada/…` local, resolvido contra essa origem confiável. A entrada inicial entrega o fragmento diretamente, preservando o fluxo da casca nativa. URLs relativas externas não são reinterpretadas como caminhos da página.

O domínio `seele.app.br` e seus subdomínios são recusados, inclusive caixa normalizada e ponto final no host. Outras origens, esquema/porta diferentes e credenciais na URL também são recusados antes de trocar convite ou recepção. Código pessoal não foi alterado.

A mensagem de endereço aposentado é “Este endereço foi aposentado. Peça um novo convite.” A entrada/vista preserva esse erro explicitamente marcado. A ponte reconhece `DominioDescontinuado` direto e como detalhe de `Convite`, conforme o contrato Rust; a tabela central também traduz o erro nativo.

## Resultados

| Bancada | Resultado |
| --- | --- |
| `chamada-web.cjs --so-origem`, após a última exigência de URL absoluta | Chromium e WebKit PASS |
| Origem nova `https://seele.sh`, fragmento local e origem HTTP local configurada | Aceitos com convite assinado real |
| 8 vetores de host/esquema/porta, 3 formas relativas e 2 URLs com credenciais | Recusados nos dois navegadores |
| Criptografia da mesma bancada | Assinatura válida aceita; 12 alterações recusadas em cada navegador |
| `chamada-web.cjs` completa, antes do endurecimento final de URL relativa | Chromium e WebKit PASS: aceite, texto bidirecional, perda RTC, nova chamada no mesmo link, recusa, cancelamento, revogação e encerramento remoto |
| `pessoal-web.cjs` | Chromium e WebKit PASS em 1280/390/320, incluindo aviso de endereço aposentado no DOM |
| `ponte-web.cjs`, depois da última mudança | 23/23 PASS |
| `git diff --check` dos arquivos JS/bancadas envolvidos | PASS |

A bancada usa o WASM atual de `apps/seele-app/ui/seele_web.wasm`; o artefato antigo em `target/wasm32-unknown-unknown/release` não contém a operação de código pessoal e não foi usado na validação aprovada.

Ao fechar a suíte da ponte, a expectativa antiga de `estado_da_sala` neutro foi substituída por prova própria de recepção real nas duas cascas: `Subindo`, `NoAr`, `Falhou` e `Desligada`. A prova revelou uma referência mutável ao estado interno. O comando agora devolve uma cópia profunda; alterar a resposta ou seu registro não modifica a resposta seguinte. As garantias dos demais neutros foram mantidas.

## Reprodução focada

Usar o Playwright já disponível no ambiente, via `PLAYWRIGHT`:

```sh
SEELE_CHAMADA_WASM=apps/seele-app/ui/seele_web.wasm node apps/seele-app/bancada/chamada-web.cjs --so-origem
node apps/seele-app/bancada/ponte-web.cjs
node apps/seele-app/bancada/pessoal-web.cjs
```

As chamadas da bancada usam encontro local já compilado, páginas reais e WASM/Ed25519 reais. A prova da mensagem na vista simula a ação recusada; a prova de tradução da ponte simula somente os formatos de erro Rust. Não há homologação externa de DNS/deploy neste registro.

## Revisão dos guardas estáticos

A suíte frontend identificou dois falsos positivos após a validação de origem. As exceções foram limitadas às linhas revisadas:

- O guarda de credencial de convite reconhece somente a linha completa da frase `DominioDescontinuado` em `ponte-web.js`. Ela não lê, armazena nem transmite convite; todos os demais caminhos seguem sujeitos à lista explícita e à verificação de presença das entradas.
- O guarda de TURN reconhece somente a linha completa de `chamada-web.js` que **recusa** `endereco.username`/`endereco.password` da URL de convite. O nome do arquivo e o conteúdo inteiro precisam coincidir, exatamente uma vez. A detecção global de `username`, `credential`, TURN e relay permanece para todas as outras linhas e arquivos.

A correspondência dessas duas exceções com a fonte foi verificada: uma ocorrência de cada. O produto JavaScript permaneceu congelado durante este ajuste de testes.

## Configuração canônica pelo WASM

O serviço agora consulta `chamada_operar` com `{acao:"configuracao"}` antes de abrir identidade ou sinalização. Os padrões `origem_dos_links`, `url_do_encontro` e `ice_servers` vêm do protocolo Rust; o JavaScript não contém cópias desses endereços. Overrides explicitamente fornecidos pela bancada continuam aceitos. A entrada pessoal usa o mesmo padrão nas cascas nativa e web.

As bancadas `chamada-web.cjs`, `entrada-prazos.cjs` e `link-na-aba-aberta.cjs` também deixaram de copiar a origem: o helper `origem-do-produto.cjs` lê `ORIGEM_DO_NAVEGADOR` em Rust, e os vetores no navegador podem consultar a configuração WASM.

`node apps/seele-app/bancada/chamada-web-configuracao.cjs`: **PASS**, com WASM/rede controlados e sem mídia. Verifica origem padrão aceita, fragmento local, outra origem recusada, endpoint padrão utilizado, overrides locais preservados e operação de configuração ausente recusada sem fallback silencioso. A configuração foi lida uma vez em cada criação do serviço. `node --check` e `git diff --check` dos trechos envolvidos: **PASS**. Essa prova não substitui o rebuild WASM nem a regressão real no navegador depois dele.

## Regressão final com WASM 0.18.0

Rodada posterior ao rebuild final: `apps/seele-app/ui/seele_web.wasm`, **2.215.066 bytes**, SHA-256 `be69ec49e09af26a45fdaa480c70d2192ad91f058a11f9a5033cde350b02eefc`. Proveniência da compilação registrada pela rodada principal em `/private/tmp/seele-018/wasm-final3.log`.

- `chamada-web.cjs` completa: **PASS em Chromium e WebKit, exit 0**. Inclui configuração WASM real, assinatura válida e 12 alterações recusadas, origens de convite, perda RTC, nova chamada no mesmo link, recusa, cancelamento antes do POST e após aceite processado, convite/aceite, texto bidirecional, chamada aceita que continua após revogação e encerramento remoto. Log: `/private/tmp/seele-018/chamada-web-config-final.log`.
- `chamada-web.cjs --so-origem`: **PASS em Chromium e WebKit, exit 0**, depois de tornar explícita a prova de defaults: serviço criado **sem overrides**, dentro de página localhost, aceita a origem canônica lida de Rust, aceita fragmento local e recusa porta diferente. O serviço local separado continua com seus overrides explícitos. Log: `/private/tmp/seele-018/chamada-web-defaults-final.log`.
- `ponte-web.cjs`: **23/23 PASS, exit 0**. Log: `/private/tmp/seele-018/ponte-web-config-final.log`.
- `chamada-web-configuracao.cjs`: **PASS** novamente, com configuração/rede controladas.

A primeira tentativa desta rodada encontrou sombreamento de variável somente na bancada (`for (const origem of [origem.replace(…)])`). A variável candidata foi renomeada e as duas rodadas acima foram concluídas depois do conserto. Nenhuma fonte JavaScript do produto foi modificada durante essa validação; fontes permanecem congeladas. Não houve captura física de voz ou execução Cargo nesta frente.

## Intermitência na bancada integrada de UI

A prova integrada é distinta da bancada de serviço acima: página inteira, CSP real, encontro Rust/WASM/RTC reais e áudio sintético; nenhum microfone físico ou som audível. Resultados observados:

| Execução | Resultado |
| --- | --- |
| `/private/tmp/seele-018/integrada-config-final.log` | Chromium PASS; WebKit **FALHOU**, timeout de 30 s aguardando o primeiro botão ATENDER, linha 116. O catch anterior não registrava fase/estado/falha das páginas, portanto não há dados suficientes para atribuir causa. |
| `/private/tmp/seele-018/integrada-config-webkit.log` | WebKit isolado, com encontro novo: PASS. |
| `/private/tmp/seele-018/integrada-config-repeticao-investigada.log` | Uma repetição completa Chromium + WebKit, sem soak e sem outra bancada na porta 19886: **PASS, exit 0**. Chromium enviou/decodificou 67/24 e 24/67 quadros; WebKit 45/2 e 2/45. Ambos exercitaram convite, aceite, chat, voz sintética, permissão tardia, saída/perda/reentrada e novo convite. |

**Intermitência permanece aberta; o retry não prova correção.** A repetição usou o catch diagnóstico já acrescentado pela rodada principal, mas ele não foi acionado porque não houve nova falha.

Revisão do isolamento: cada motor cria um navegador novo; anfitrião e convidado usam contextos separados, sem compartilhar IndexedDB entre eles. Na rodada completa, os motores compartilham o mesmo processo de encontro e origem local, portanto estado de sinalização e limites por IP/prefixo persistem entre motores. Nenhuma recusa de limite, erro HTTP ou falha de polling foi demonstrada no material da primeira ocorrência. Não há evidência para atribuir a falha a rate limit ou estado compartilhado.

Foi revisada uma hipótese de corrida entre abertura assíncrona do convite e clique em ENTRAR: a vista abre antes dos awaits de preparação, e a ação reaplica convite somente quando o serviço está livre. Essa leitura isolada não demonstra um interleaving real que explique a falha; recepção cacheada e microtasks podem concluir antes do próximo clique. A hipótese não foi promovida a defeito confirmado e nenhuma fonte do produto foi alterada.

Se voltar a ocorrer, o catch atual deve registrar fase, estado e mensagem das duas páginas; a captura de status HTTP/polling e da operação de sinalização, sem payload de convite ou chave, permitirá distinguir preparação da UI, recusa do encontro e entrega de evento. Nesta investigação foi executada somente a repetição completa solicitada, sem soak adicional. A porta e o processo da bancada foram liberados ao final.

## Fechamento da corrida de preparo do convite

A investigação posterior confirmou um defeito concreto no produto: com a resposta real de `Registrar` da recepção automática retida, clicar em ENTRAR tentava `abrirConvite` enquanto o serviço ainda estava criando essa recepção. O serviço recusava a operação antes de enviar a chamada; o anfitrião nunca recebia ATENDER. A hipótese acima deixou de ser apenas leitura de código: a barreira de rede reproduziu a falha antes da correção (`/private/tmp/seele-018/integrada-race-controlada-antes.log`).

`pessoal-web-entrada.js` agora guarda a promessa do preparo e seu token. ENTRAR abre a saída de áudio sincronamente no gesto e aguarda essa promessa antes de abrir/chamar o convite. VOLTAR, criação de outro convite e nova abertura invalidam o token; uma continuação antiga não alcança a nova vista. A recepção pessoal pode continuar depois de VOLTAR, conforme o comportamento existente.

| Prova posterior à correção | Resultado |
| --- | --- |
| Registro real retido + ENTRAR antecipado, WebKit focado | PASS: `/private/tmp/seele-018/integrada-race-webkit-fix.log` |
| Mutante local retirando somente a espera pelo preparo (`--reverter-preparo`) | Falhou na asserção de clique cedo recusado: `/private/tmp/seele-018/integrada-race-mutante.log`. Não é uma rodada aprovada. |
| VOLTAR com ENTRAR aguardando Registrar; liberar resposta tardia | Chromium e WebKit PASS, exit 0: `/private/tmp/seele-018/integrada-race-cancelamento-final.log`. Observador da bancada comprovou **zero** chamadas a `abrirConvite` e `chamar` após cancelamento, vista fechada e nova abertura com CRIAR CONVERSA. |
| Integrada completa com Registrar retido, depois da correção | Chromium e WebKit PASS, exit 0: `/private/tmp/seele-018/integrada-race-completa-fechamento.log` e `/private/tmp/seele-018/integrada-race-completa-final.log`. Na última: Chromium 64/25 e 25/65 quadros enviados/decodificados; WebKit 66/25 e 25/66. Inclui chat bidirecional, gestos de áudio ativos, voz sintética nos dois sentidos, permissão tardia, saída/perda/reentrada e novo convite. |

As rodadas completas aprovadas exercitam a barreira determinística; não dependem de retry para corrigir a corrida. Usaram o mesmo WASM final de 2.215.066 bytes e SHA-256 `be69ec49e09af26a45fdaa480c70d2192ad91f058a11f9a5033cde350b02eefc`, encontro local e origem da fixture explicitamente configurada como `location.origin`. Sem regeneração de WASM, Cargo, produção ou captura física nesta frente.

Uma primeira tentativa do novo observador de cancelamento copiou a promessa do serviço antes do `await`, impedindo a criação do link na própria bancada (`integrada-race-cancelamento-fechamento.log`). O observador foi corrigido para aguardar a fábrica assíncrona antes de observar os métodos. Os PASS de cancelamento e da integrada final acima são posteriores a esse conserto.

A falha distinta de campo inicial “Seu nome” invisível ocorreu em `/private/tmp/seele-018/integrada-race-regressao-final.log`, antes de criar o convite do anfitrião. O log não registra helper mobile nem estado da vista; não demonstra se o clique aconteceu sem `CELULAR.ajudante("conversaPessoal")`. O optional chaining nesse botão é uma hipótese de no-op, sem reprodução controlada. As rodadas completas e de cancelamento aprovadas não a repetiram; isso **não comprova correção** dessa ocorrência. A bancada agora registra clique, disponibilidade do helper, fase, visibilidade e erros se o formulário voltar a falhar. Esse limite permanece aberto, separado da corrida de registro confirmada e corrigida.

Reprodução local (definir `PLAYWRIGHT` para uma instalação disponível):

```sh
SEELE_PROVA_REGISTRO_PENDENTE=1 node apps/seele-app/bancada/pessoal-web-integrada.cjs
SEELE_PROVA_REGISTRO_PENDENTE=1 node apps/seele-app/bancada/pessoal-web-integrada.cjs --cancelar-preparo
```

`node --check` dos dois arquivos envolvidos e `git diff --check`: PASS. **JavaScript do produto congelado após esta correção**, informado à rodada principal antes de seus builds finais. Não houve publicação nem commit nesta frente.

### Ensaio focado do formulário inicial WebKit

`ENGINE=webkit node apps/seele-app/bancada/pessoal-web-integrada.cjs --so-formulario`: **10/10 páginas novas PASS, exit 0**, com um contexto novo por página, sem retry do clique, iniciar chamada, microfone ou soak. Fixture e encontro locais reutilizados. Log com eventos DOM: `/private/tmp/seele-018/integrada-formulario-webkit-10.log`.

Nas dez páginas, o helper `conversaPessoal.abrir` já era uma função durante pointerdown, pointerup e click. O click chegou à captura e à bolha; na bolha a vista estava aberta, fase `livre` e campo de nome visível. Houve uma remoção/redesenho do botão por página **antes** de pointerdown. Não houve remoção registrada entre pointerdown e pointerup/click. Assim, o redesenho inicial existe, mas este ensaio não reproduziu perda do clique nem demonstrou que ele causou a ocorrência anterior. A hipótese de helper ausente também não ocorreu nessas dez páginas.

Classificação: ocorrência histórica isolada, **não reproduzida neste limite de dez páginas**, sem causa comprovada e sem alegação de correção. Produto permaneceu congelado; somente bancada e evidência foram ampliadas.
